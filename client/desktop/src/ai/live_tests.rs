// SPDX-License-Identifier: GPL-3.0-only
//! The gated live AI suite: the A1 stages proven against the real model
//! endpoint, over the host's own seam implementations rather than any test
//! double. The generation seam answers parseable rules from a sample
//! sentence, the categorisation stage lands a closed-set category, costs
//! land in `llm_calls`, and the live-mode eval scenarios hold against
//! model answers. The harness-backed tests (the cold judge on the seeded
//! note, the incoming pass over a real refresh) live in the `harness`
//! child module, gated the same way plus the mail harness.
//!
//! The gate is `ASTRA_LIVE_LLM=1` plus the connection environment the
//! background client reads (`ASTRA_LLM_*` tier lists, one provider key,
//! the optional base override); an open gate with an unusable connection
//! fails loudly rather than skipping, because somebody asked for the live
//! run. The recorded invocation runs this suite one test at a time
//! (`-- --nocapture --test-threads=1`): the tests share one model endpoint
//! and one live server, and serial calls keep a rate limit from failing a
//! neighbour.
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! export ASTRA_LIVE_LLM=1
//! export ASTRA_LLM_DEFAULT=anthropic:deepseek-flash
//! export ASTRA_LLM_API_KEY_ANTHROPIC=<the key>
//! export ASTRA_LLM_BASE_URL_ANTHROPIC=https://api.deepseek.com/anthropic
//! cargo test -p astra-desktop ai::live_tests -- --nocapture --test-threads=1
//! ```

// The harness-backed half: its own file so this one stays under the
// 500-line cap the gate enforces. The explicit path because this file is
// loaded through `#[path]` and so owns no directory a bare `mod` would
// resolve into.
#[path = "live_tests/harness.rs"]
mod harness;

use std::sync::Arc;

use astra_ai::{CorrelationId, TierTable};
use astra_app::{categorise, prompt_to_rules, Action, AiComplete, AiJudge, Category, MailContext};
use engine_api::AccountId;

use super::seams::{LlmAiComplete, LlmAiJudge};
use super::LlmClient;
use crate::ai::fixture::{self, Ledger};

/// The live connection: the client built from the real environment, over
/// its own scratch ledger. `None` skips (the gate is closed); an open gate
/// with an unusable connection panics, naming what is missing.
pub(super) fn live(tag: &str) -> Option<(Arc<LlmClient>, Ledger)> {
    if std::env::var("ASTRA_LIVE_LLM").ok().as_deref() != Some("1") {
        return None;
    }
    let registry = TierTable::from_env().unwrap_or_else(|err| {
        panic!("the live gate is open but the tier table is unusable: {err}")
    });
    let keyed = ["ASTRA_LLM_API_KEY_OPENAI", "ASTRA_LLM_API_KEY_ANTHROPIC"]
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()));
    assert!(
        keyed,
        "the live gate is open but no ASTRA_LLM_API_KEY_* key is set"
    );
    let ledger = Ledger::open(tag);
    let client = Arc::new(LlmClient::new(fixture::http(), registry, ledger.store()));
    Some((client, ledger))
}

/// Generation live: a sample sentence through the real completion seam
/// comes back as parseable rules, every action inside the closed set (the
/// parse enforces it; the assertions read the shapes a caller relies on).
#[test]
fn prompt_to_rules_live_generates_parseable_rules() {
    let Some((client, ledger)) = live("live-generate") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let seam = LlmAiComplete::new(client);
    let correlation = CorrelationId::new("corr-live-generate".to_string());

    let rules = fixture::runtime()
        .block_on(|| {
            prompt_to_rules(
                "Mark messages about the weekly deployment report as read, and flag \
                 anything from Ada.",
                &correlation,
                &seam,
            )
        })
        .expect("the live generation call parses");

    assert!(
        !rules.is_empty(),
        "the sample sentence yields at least one rule"
    );
    for rule in &rules {
        assert!(!rule.name.trim().is_empty(), "a generated rule is named");
        assert!(
            !rule.condition.trim().is_empty(),
            "a generated rule carries a condition sentence"
        );
        assert!(
            !rule.actions.is_empty()
                && rule
                    .actions
                    .iter()
                    .all(|action| Action::ALL.contains(action)),
            "every generated action is inside the closed set: {:?}",
            rule.actions
        );
        assert!(rule.enabled, "a generated rule arrives enabled");
    }
    let rows = ledger.rows();
    assert!(
        rows.iter()
            .any(|row| row.correlation == "corr-live-generate" && row.outcome == "ok"),
        "the generation call landed its ledger row: {rows:?}"
    );
}

/// Categorisation live: a clear marketing mailing through the real
/// completion seam lands one of the five closed-set words and its 0003
/// row records the decision.
#[test]
fn categorise_live_assigns_a_closed_set_category() {
    let Some((client, ledger)) = live("live-categorise") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let seam = LlmAiComplete::new(client);
    let correlation = CorrelationId::new("corr-live-categorise".to_string());
    let mail = MailContext {
        account: AccountId::try_from("work").expect("the account id"),
        key: engine_api::ProviderKey::new("m-live-1").expect("the key"),
        from_name: "The Weekly Launch".to_owned(),
        from_address: "digest@news.example.test".to_owned(),
        subject: "The Weekly Launch, issue 42".to_owned(),
        body: "You are receiving this because you subscribed at news.example.test. This \
               week: our product roundup, three upcoming events, and a word from our \
               sponsor."
            .to_owned(),
    };

    let store = ledger.store();
    let category = fixture::runtime()
        .block_on(|| categorise(&mail, &correlation, &seam, &store))
        .expect("the live categorisation call decides");

    assert!(
        Category::ALL.contains(&category),
        "the decided category is one of the closed set: {category}"
    );
    let rows = store
        .ai_results_for_message(mail.account.as_str(), mail.key.as_str())
        .expect("the results read");
    assert!(
        rows.iter().any(|row| row.stage == "categorise"
            && row.verdict == "ok"
            && row.category.as_deref() == Some(category.as_str())),
        "the decision landed its 0003 row: {rows:?}"
    );
}

/// Costs live: the calls the suite makes land ledger rows with sane token
/// counts (positive, bounded; never exact pins) and truthful outcomes.
/// The verified endpoint occasionally answers a completion with a textless
/// 2xx envelope (its thinking consumed the token budget); the client
/// classifies that as a retryable bad response, the ledger records the
/// error row honestly, and the test retries the completion a bounded
/// number of times before asserting over the successful attempts.
#[test]
fn llm_calls_observe_sane_costs_live() {
    let Some((client, ledger)) = live("live-costs") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let complete = LlmAiComplete::new(Arc::clone(&client));
    let judge = LlmAiJudge::new(Arc::clone(&client));
    let correlation = CorrelationId::new("corr-live-costs".to_string());

    fixture::runtime().block_on(|| async {
        let mut completion_ok = false;
        for _ in 0..3 {
            if complete
                .complete(
                    astra_ai::LlmUseCase::Summarise,
                    "Reply with exactly one word: ready.",
                    &correlation,
                )
                .await
                .is_ok()
            {
                completion_ok = true;
                break;
            }
        }
        assert!(completion_ok, "the completion never answered with text");
        judge
            .judge(
                astra_ai::JudgeUseCase::JudgeColdEmail,
                "Does the message in the context look like unsolicited cold outreach?",
                "From: Ada <ada@example.test>\nSubject: Notes\n\nThanks for the call today.",
                &correlation,
            )
            .await;
    });

    let rows: Vec<_> = ledger
        .rows()
        .into_iter()
        .filter(|row| row.correlation == "corr-live-costs")
        .collect();
    let succeeded: Vec<_> = rows
        .iter()
        .filter(|row| row.outcome == "ok" || row.outcome.starts_with("judge:"))
        .collect();
    assert!(
        succeeded.len() >= 2,
        "the completion and the judge both landed successful rows: {rows:?}"
    );
    for row in &succeeded {
        assert!(
            row.prompt_tokens > 0 && row.prompt_tokens < 100_000,
            "the prompt token count is sane, not exact: {row:?}"
        );
        assert!(
            row.completion_tokens > 0 && row.completion_tokens < 10_000,
            "the completion token count is sane, not exact: {row:?}"
        );
        assert!(
            !row.model.is_empty(),
            "the answering model is named: {row:?}"
        );
    }
}

/// The live-mode eval scenarios hold against model answers: the same
/// fixtures the offline battery pins, driven through the real client
/// adapter, the harness the rulebook's AI rule rests on.
#[test]
fn live_eval_scenarios_drive_green_over_the_real_client() {
    let Some((client, _ledger)) = live("live-evals") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let complete = LlmAiComplete::new(Arc::clone(&client));
    let judge = LlmAiJudge::new(client);
    let scenarios = astra_app::evals::scenarios();
    let live: Vec<_> = scenarios
        .iter()
        .filter(|scenario| scenario.mode == astra_app::evals::Mode::Live)
        .collect();
    assert!(
        !live.is_empty(),
        "the evals directory holds live-mode scenarios"
    );

    fixture::runtime().block_on(|| async {
        for scenario in live {
            astra_app::evals::pin_version(scenario)
                .unwrap_or_else(|err| panic!("{}: the version pin: {err}", scenario.id));
            astra_app::evals::drive(scenario, &complete, &judge)
                .await
                .unwrap_or_else(|err| panic!("{}: {err}", scenario.id));
        }
    });
}
