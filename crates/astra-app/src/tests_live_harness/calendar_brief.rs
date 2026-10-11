// SPDX-License-Identifier: GPL-3.0-only
//! The relayed meeting brief proof, against the running harness plus the
//! relay environment the tier registry reads: the controller runs it with
//! the relayed key, and without the env it skips cleanly. The gate is
//! `ASTRA_LLM_ECONOMY` (the Economy tier's env, the tier the meeting
//! brief's use case resolves through); an open gate with an unusable
//! connection fails loudly, never silently, because somebody asked for
//! the live run.
//!
//! The suite cannot reuse the desktop host's background client (the host
//! depends on this crate, so the dependency cannot turn around), so the
//! proof dials the relay itself: a minimal dual-protocol completion seam
//! over the same wire shapes the host's client speaks (the openai chat
//! completions and anthropic messages endpoints, the same env names for
//! keys and base overrides), one attempt per chain entry with the chain's
//! own fallback. Everything else is the product's own path: the seeded
//! day syncs through the real CalDAV dial and the brief generates through
//! the app core's own drive, the registry's tier, and the redacted
//! versioned prompt.

use std::sync::Arc;

use astra_ai::{AiStore, CorrelationId, DlpPolicy, LlmUseCase, ModelEntry, TierTable};
use async_trait::async_trait;
use engine_api::CalendarDate;

use crate::dispatch_fakes::ai::FixtureJudge;
use crate::{AiPass, CalendarIntent, Intent, MeetingBriefState};

use super::calendar_proof::{dav, event_ics, range, rows_of, run_token, suite_apps};
use super::harness;

/// The two wire protocols a provider prefix selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    /// `POST {base}/chat/completions`, bearer auth.
    OpenAi,
    /// `POST {base}/v1/messages`, `x-api-key` plus `anthropic-version`.
    Anthropic,
}

impl Protocol {
    /// The provider's default endpoint base.
    fn base(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com",
        }
    }
}

/// The completion seam over the relay the tier registry names: the minimal
/// dual-protocol client the relayed brief proof dials. One attempt per
/// resolved chain entry; a failure advances to the next entry.
struct RelayComplete {
    /// The HTTP client over the engine's TLS policy.
    http: reqwest::Client,
    /// The MeetingBrief chain's entries: protocol, model, URL, key.
    entries: Vec<(Protocol, String, String, String)>,
}

#[async_trait]
impl crate::AiComplete for RelayComplete {
    async fn complete(
        &self,
        _use_case: LlmUseCase,
        prompt: &str,
        _correlation: &CorrelationId,
    ) -> Result<String, String> {
        let mut failure = "the chain held no entry".to_owned();
        for (protocol, model, url, key) in &self.entries {
            let content = serde_json::json!({ "role": "user", "content": prompt });
            let (url, mut headers, body) = match protocol {
                Protocol::OpenAi => (
                    format!("{}/chat/completions", url.trim_end_matches('/')),
                    vec![("authorization", format!("Bearer {key}"))],
                    serde_json::json!({ "model": model, "messages": [content] }).to_string(),
                ),
                Protocol::Anthropic => {
                    let mut base = url.trim_end_matches('/');
                    if let Some(stripped) = base.strip_suffix("/v1") {
                        base = stripped.trim_end_matches('/');
                    }
                    (
                        format!("{base}/v1/messages"),
                        vec![
                            ("x-api-key", key.clone()),
                            ("anthropic-version", "2023-06-01".to_owned()),
                        ],
                        serde_json::json!({
                            "model": model,
                            "max_tokens": 1024,
                            "thinking": { "type": "disabled" },
                            "messages": [content],
                        })
                        .to_string(),
                    )
                }
            };
            headers.push(("content-type", "application/json".to_owned()));
            let mut request = self.http.post(&url);
            for (name, value) in &headers {
                request = request.header(*name, value.as_str());
            }
            let response = request
                .body(body)
                .send()
                .await
                .map_err(|err| err.to_string())?;
            let status = response.status();
            let text = response.text().await.map_err(|err| err.to_string())?;
            if !status.is_success() {
                failure = format!("the relay answered {status}: {text}");
                continue;
            }
            let envelope: serde_json::Value =
                serde_json::from_str(&text).map_err(|err| err.to_string())?;
            let answer = match protocol {
                Protocol::Anthropic => envelope["content"]
                    .as_array()
                    .and_then(|blocks| {
                        blocks
                            .iter()
                            .find(|block| block["type"] == "text")
                            .and_then(|block| block["text"].as_str())
                    })
                    .map(str::to_owned),
                Protocol::OpenAi => envelope
                    .pointer("/choices/0/message/content")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            };
            if let Some(answer) = answer {
                return Ok(answer);
            }
            failure = "the relay's 2xx envelope carried no text".to_owned();
        }
        Err(failure)
    }
}

/// The relayed meeting brief over the seeded day: the generation names the
/// day's meetings through the real CalDAV sync, the registry's tier, and
/// the relay the env names; the emptied day then answers the defined
/// no-meetings state, spending no call. Skips cleanly without the relay
/// env.
#[tokio::test]
async fn the_meeting_brief_names_the_seeded_day_over_the_relay() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    if std::env::var("ASTRA_LLM_ECONOMY")
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        eprintln!("skipping live relay test: ASTRA_LLM_ECONOMY is not set");
        return;
    }
    let registry =
        TierTable::from_env().expect("the relay gate is open, so the tier table must parse");
    let chain = registry
        .resolve(LlmUseCase::MeetingBrief)
        .expect("the meeting brief chain resolves");
    let mut entries = Vec::new();
    for entry in chain {
        let ModelEntry { provider, model } = entry.clone();
        let (protocol, prefix) = match provider.as_str() {
            "openai" => (Protocol::OpenAi, "OPENAI"),
            "anthropic" => (Protocol::Anthropic, "ANTHROPIC"),
            other => panic!("the relay names a provider this proof cannot dial: {other}"),
        };
        let key = std::env::var(format!("ASTRA_LLM_API_KEY_{prefix}")).unwrap_or_else(|_| {
            panic!("the relay gate is open but ASTRA_LLM_API_KEY_{prefix} is unset")
        });
        let base = std::env::var(format!("ASTRA_LLM_BASE_URL_{prefix}"))
            .unwrap_or_else(|_| protocol.base().to_owned());
        entries.push((protocol, model, base, key));
    }
    assert!(
        !entries.is_empty(),
        "the relay gate is open but the meeting brief chain holds no entry"
    );
    let tls = engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system())
        .expect("the relay TLS policy builds");
    let http = tls
        .reqwest_builder()
        .build()
        .expect("the relay client builds");

    let token = run_token();
    let morning = format!("Harness brief standup {token}");
    let afternoon = format!("Harness brief planning {token}");
    let uids: Vec<String> = ["standup", "planning"]
        .iter()
        .map(|kind| format!("{kind}-{token}@astra.local"))
        .collect();
    let seeds = [
        event_ics(
            &uids[0],
            &morning,
            "DTSTART:20261105T090000Z",
            "DTEND:20261105T093000Z",
            &[],
        ),
        event_ics(
            &uids[1],
            &afternoon,
            "DTSTART:20261105T140000Z",
            "DTEND:20261105T150000Z",
            &[],
        ),
    ];
    for (uid, bytes) in uids.iter().zip(&seeds) {
        assert_eq!(dav("PUT", &user, uid, &password, bytes), 201);
    }

    let (_mail, calendar, first, _second) = suite_apps(&host, &port, &user, &password).await;
    let app = calendar.with_ai_pass(Some(AiPass::new(
        Box::new(FixtureJudge::scripted(&[])),
        Box::new(RelayComplete { http, entries }),
        Arc::new(AiStore::open_in_memory().expect("the scratch ledger")),
        DlpPolicy::Allow,
    )));
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        rows_of(&app.calendar(), &token).len(),
        2,
        "the seeded day renders"
    );

    let day = CalendarDate::new(2026, 11, 5).expect("the brief day");
    let view = app
        .generate_meeting_brief(&first, day)
        .await
        .expect("the brief generates");
    assert_eq!(view.state, MeetingBriefState::Generated);
    assert_eq!(
        view.meetings.len(),
        2,
        "the view carries the day's meetings"
    );
    let brief = view.brief.to_lowercase();
    assert!(
        brief.contains("standup"),
        "the brief names the standup: {brief}"
    );
    assert!(
        brief.contains("planning"),
        "the brief names the planning: {brief}"
    );

    // The zero-spend state, live: an emptied day answers no meetings and
    // makes no call.
    for uid in &uids {
        assert_eq!(dav("DELETE", &user, uid, &password, ""), 204);
    }
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let view = app
        .generate_meeting_brief(&first, day)
        .await
        .expect("the emptied day reads");
    assert_eq!(view.state, MeetingBriefState::NoMeetings);
    assert!(view.brief.is_empty() && view.meetings.is_empty());
}
