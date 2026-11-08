// SPDX-License-Identifier: GPL-3.0-only
//! The gated live proof of the assistant chat itself (the SP2y spec's
//! closing pin): one real turn over the REAL sidecar bundle at the pi
//! harness, the live model at the chat tier, the spawned real astra-cli
//! against a harness-backed file-backed engine, and an armed short tool
//! budget. The turn searches the proof's seeded distinguishable mail,
//! acts on the row through its confirm gate (the test answers the card
//! through the relay's permission response), keeps calling tools until
//! the budget strips them, and still lands done with both thread rows
//! recorded. The assertions pin the ROUTE facts only, never the model's
//! wording: the search's page shape answered over the seed, the act
//! verb's ask approved with its audit rows, the budget-spent note
//! crossing before the done envelope, and the run's metadata stamped.
//!
//! The gates are the family's: `ASTRA_LIVE_LLM=1` plus the connection
//! environment for the model, `ASTRA_LIVE_HARNESS=1` plus the harness
//! environment for the mail, and the short budget override
//! `ASTRA_AGENT_TOOL_BUDGET_MS` armed in the shell (the recorded value is
//! 45000; the spawn env allowlist carries it into the sidecar). An open
//! gate with an unusable connection, no built astra-cli, or an unarmed
//! budget fails loudly rather than skipping. The recorded invocation
//! runs the suite serially, the family's own discipline.
//!
//! A model that stops calling tools before the budget bites, refuses the
//! route, or a flaky endpoint fails the test; the rerun discipline is the
//! operator's. The opening sweep withdraws a previous interrupted run's
//! seeded copies, so a fresh invocation starts from a known harness.
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! export ASTRA_LIVE_LLM=1
//! export ASTRA_LLM_DEFAULT=anthropic:deepseek-flash
//! export ASTRA_LLM_API_KEY_ANTHROPIC=<the key>
//! export ASTRA_LLM_BASE_URL_ANTHROPIC=https://api.deepseek.com/anthropic
//! export ASTRA_AGENT_TOOL_BUDGET_MS=45000
//! cargo test -p astra-desktop ai::live_chat -- --nocapture --test-threads=1
//! ```
//!
//! What the proof pins versus what the evals own: the skipped outcome a
//! past-budget call answers, the watchdog's done envelope, the search's
//! no-ask read, the rejection path where nothing executes, and the
//! read's truncation cap are the offline evals' pins (the budget and
//! agent-tools fixtures); this proof adds the one fact only a live model
//! can give, that the whole route holds together end to end against real
//! wording and a real clock.

// The harness-backed support: its own file so this one stays under the
// 500-line cap the gate enforces. The explicit path because this file is
// loaded through `#[path]` and so owns no directory a bare `mod` would
// resolve into.
#[path = "live_chat/harness.rs"]
mod harness;

use std::{path::Path, sync::Arc};

use astra_ai::{AiStore, ChatId, CorrelationId, NewChat};

use super::live_tests::live;
use crate::agent::{
    chat_model_config, default_policy, find_node, resolve_cli_path, spawn_sidecar, testkit,
    PendingApproval, PermissionLevel, SidecarConfig, SidecarRelay, StreamPayload as S,
    AGENT_PIPELINE_VERSION,
};
use crate::ai::fixture;

use harness::{
    assist_app, delete_subject_mail, dial, document, ensure_bundle, harness, refresh,
    seed_is_searchable, write_account_index, SEED_SUBJECT, SEED_WORDS,
};

/// The correlation id the turn rides: fixed so every assertion can name
/// it, the way `prompt` names a caller's id.
const CORR: &str = "corr-live-a2-chat";

/// The chat the turn records into, fixed the same way.
const CHAT: &str = "live-chat-proof";

/// The confirm-gated act verbs the proof accepts, each beside the CLI
/// verb word its own audit row carries: the prompt steers the model to
/// mark the seed read, and which verb it actually picks stays the model's
/// choice (a route fact, never wording).
const ACT_TOOLS: [(&str, &str); 4] = [
    ("mark_read", "messages mark-read"),
    ("set_flagged", "messages set-flagged"),
    ("archive_message", "messages archive"),
    ("move_to_trash", "messages trash"),
];

/// The turn's instruction, engineered for the route the proof pins: name
/// the seed's subject so the search has one row to find, name the act so
/// the confirm gate crosses, order persistence so the budget bites inside
/// the turn, and fence the drafts so the confirm cards belong to the act.
const PROMPT: &str = "Tidy my mailbox with your mail tools, step by step. \
                      Step 1: search all of my mail for the message with the subject \
                      'Harbour lantern bloom note'. Step 2: mark that message as read with \
                      the mark_read tool. Step 3: keep using your mail tools to search and \
                      read more of my mail, over and over, until the tools answer that the \
                      turn's tool budget is spent and skip the call; only then stop \
                      calling tools. Step 4: reply with one short sentence about what you \
                      did. Do not send mail and do not draft anything.";

/// The armed short budget, checked loudly: an open gate with no override
/// would run the turn under the twelve-minute default, never lose its
/// tools inside the proof's patience, and hang on the watchdog instead.
fn armed_budget_ms() -> u64 {
    let raw = std::env::var("ASTRA_AGENT_TOOL_BUDGET_MS").unwrap_or_else(|_| {
        panic!(
            "the live chat proof needs ASTRA_AGENT_TOOL_BUDGET_MS armed in the shell (the \
             recorded value is 45000) or the turn cannot lose its tools inside the proof's \
             patience"
        )
    });
    let ms: u64 = raw
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("ASTRA_AGENT_TOOL_BUDGET_MS must be milliseconds: {raw:?}"));
    assert!(
        (15_000..=120_000).contains(&ms),
        "an armed budget of {ms} ms cannot both run the search and the act and bite inside \
         the proof's patience; the recorded value is 45000"
    );
    ms
}

/// One whole live chat turn: the real relay over the real sidecar bundle
/// and the real astra-cli, the live model at the chat tier, the harness
/// account's mail in the same file-backed engine the spawned CLI reads,
/// and the armed short budget riding the spawn env allowlist.
#[test]
fn a_chat_turn_searches_acts_and_lands_done_at_an_armed_budget() {
    let Some((client, _ledger)) = live("live-a2-chat") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    armed_budget_ms();
    assert!(
        PROMPT.contains(SEED_SUBJECT),
        "the prompt names the seed's subject"
    );
    let node = find_node().expect("a node >= 22 is on PATH for the chat live proof");
    let bundle = ensure_bundle(&node);
    let cli = resolve_cli_path();
    assert!(
        !cli.is_empty() && Path::new(&cli).is_file(),
        "the real astra-cli binary must be built (cargo build -p astra-cli) for the chat \
         live proof; resolved {cli:?}"
    );
    let model = chat_model_config(&client)
        .expect("the live gate is open but the chat tier resolves no endpoint");

    let data_dir = testkit::scratch("live-a2-chat");
    let store = Arc::new(AiStore::open(&data_dir.join("ai.sqlite")).expect("the store opens"));

    fixture::runtime().block_on(|| async {
        let (account, providers) = dial(&host, &port, &user, &password).await;
        let engine = Arc::new(
            engine_api::Engine::open(data_dir.join("astra.sqlite"))
                .expect("the file-backed engine opens"),
        );
        let app = assist_app(Arc::clone(&engine), account.clone(), providers);
        // A second dial's first provider is the sweep's own handle: the app
        // keeps its providers private, and the cleanup needs one the test
        // holds.
        let (_scratch_account, cleanup_providers) = dial(&host, &port, &user, &password).await;
        let cleanup = cleanup_providers.into_iter().next().expect("a provider");
        write_account_index(&data_dir, &account);

        // A known harness, the seed, and the preflight that keeps a failed
        // turn diagnosable: the store itself must answer the seeded row
        // before any model is asked to.
        refresh(&app).await;
        delete_subject_mail(&engine, &cleanup, &account, &[SEED_SUBJECT]).await;
        refresh(&app).await;
        app.dispatch(astra_app::Intent::SubmitRichMail {
            from: account.clone(),
            to: "astra1@example.test".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: SEED_SUBJECT.to_owned(),
            document: document(SEED_WORDS),
            blobs: Vec::new(),
            crypto: astra_app::CryptoFlags::default(),
            composition: None,
        })
        .await
        .expect("the seed send");
        refresh(&app).await;
        assert!(
            seed_is_searchable(&engine, &account).await,
            "the store answers the seeded row before the model runs"
        );

        // The relay, built the way the host builds its own: the pi harness,
        // the real CLI, the in-band model config (the key never rides the
        // spawn environment), and the fail-closed policy table.
        let sink = Arc::new(testkit::CollectingSink::default());
        let mut model_configs = std::collections::BTreeMap::new();
        model_configs.insert("chat".to_string(), model);
        let spawn = spawn_sidecar(node.clone(), bundle.clone(), data_dir.clone(), Vec::new());
        let relay = SidecarRelay::new(
            SidecarConfig {
                node_path: node.clone(),
                script: bundle,
                data_dir: data_dir.clone(),
                cli_path: cli,
                harness: "pi".to_string(),
                mock_fixture: None,
                model_configs,
                policy: default_policy(),
                restart_max: 3,
                backoff_base_ms: 50,
                config_error: None,
                spawn,
            },
            sink.clone(),
            Arc::clone(&store),
        );

        let chat = store
            .insert_chat(&NewChat {
                id: ChatId::new(CHAT.to_owned()),
                title: "The live chat proof".to_owned(),
            })
            .expect("the chat lands");
        // The seeded row's keys (its INBOX delivery and its Sent filing
        // alike): the approval discipline below answers approved exactly
        // the acts that name one of them, so a willful model cannot move
        // harness mail the cleanup does not withdraw. Everything else is
        // refused, the fail-closed leg where nothing executes.
        let seeded_keys: Vec<String> = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .into_iter()
            .filter(|message| message.envelope.subject.as_deref() == Some(SEED_SUBJECT))
            .map(|message| message.id.key().as_str().to_owned())
            .collect();
        assert!(!seeded_keys.is_empty(), "the seed landed with keys");
        let approves = |ask: &PendingApproval| {
            ACT_TOOLS.iter().any(|(tool, _)| *tool == ask.tool)
                && ask.args["key"]
                    .as_str()
                    .is_some_and(|key| seeded_keys.contains(&key.to_owned()))
        };
        let prompt_id = relay
            .prompt(
                PROMPT,
                Some(CorrelationId::new(CORR.to_owned())),
                Some(&chat.id),
            )
            .await
            .expect("the prompt is sent");

        // Drive the turn the way the panel would: every approval card is
        // answered through the relay's permission response (the act on the
        // seeded row approved, anything else refused), until the turn's
        // own done envelope crosses.
        harness::drive_turn(&relay, &sink, &prompt_id, approves).await;

        // The whole stream is one turn, and it never errored.
        let events = sink.agent_events.lock().unwrap().clone();
        assert!(
            events
                .iter()
                .all(|event| event.correlation.as_deref() == Some(CORR)),
            "every event carries the turn's correlation id: {events:?}"
        );
        assert!(
            events
                .iter()
                .all(|event| !matches!(event.event, S::Error { .. })),
            "the turn landed done, never an error: {events:?}"
        );

        // The search's route fact: a search_messages call crossed and its
        // ok result carries the CLI envelope's page shape over the seed.
        let call_names: std::collections::HashMap<String, String> = events
            .iter()
            .filter_map(|event| match &event.event {
                S::ToolCall {
                    name, tool_call_id, ..
                } => Some((tool_call_id.clone(), name.clone())),
                _ => None,
            })
            .collect();
        assert!(
            call_names.values().any(|name| name == "search_messages"),
            "the model searched the mail: {events:?}"
        );
        let search_output = events
            .iter()
            .find_map(|event| match &event.event {
                S::ToolResult {
                    tool_call_id,
                    status,
                    output,
                    ..
                } if status == "ok"
                    && call_names.get(tool_call_id).map(String::as_str)
                        == Some("search_messages") =>
                {
                    output.clone()
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("the search's ok result crossed: {events:?}"));
        let envelope: serde_json::Value = serde_json::from_str(&search_output)
            .unwrap_or_else(|e| panic!("the search result is the CLI envelope: {e}"));
        assert!(
            envelope["ok"].as_bool() == Some(true),
            "the search succeeded: {envelope}"
        );
        let page = &envelope["data"];
        let rows = page["rows"].as_array().cloned().unwrap_or_default();
        assert!(
            page["total"].is_number()
                && page["offset"].is_number()
                && page["has_more"].is_boolean(),
            "the page answers the search shape (rows, total, offset, has_more): {page}"
        );
        assert!(!rows.is_empty(), "the page answered rows: {page}");
        assert!(
            rows.iter().any(|row| row["subject"]
                .as_str()
                .is_some_and(|subject| subject.contains("lantern"))),
            "one row is the seeded mail: {rows:?}"
        );

        // The act's route fact: a confirm-gated act verb asked for the
        // seeded row, carried the turn's correlation, and was approved
        // through the relay.
        let requests = sink.requests.lock().unwrap().clone();
        let act_ask = requests
            .iter()
            .find(|ask| approves(ask))
            .unwrap_or_else(|| {
                panic!("a confirm-gated act verb asked for the seeded row: {requests:?}")
            });
        assert_eq!(
            act_ask.level,
            PermissionLevel::Confirm,
            "the act runs behind the confirm gate: {act_ask:?}"
        );
        assert_eq!(
            act_ask.args["account"].as_str(),
            Some(account.as_str()),
            "the act names the harness account: {act_ask:?}"
        );
        assert_eq!(
            act_ask.correlation.as_deref(),
            Some(CORR),
            "the approval card carries the turn's correlation: {act_ask:?}"
        );
        let settled_approved = {
            let resolved = sink.resolved.lock().unwrap();
            resolved
                .iter()
                .any(|settle| settle.id == act_ask.id && settle.approved)
        };
        let settled_view = format!("{:?}", sink.resolved.lock().unwrap());
        assert!(
            settled_approved,
            "the act's ask settled approved beside the ask: {settled_view}"
        );

        // The budget's route fact: the spent note crossed before the turn
        // settled, and the settle is the done envelope itself.
        let spent_at = events.iter().position(
            |event| matches!(&event.event, S::State { state } if state == "budget-spent"),
        );
        let done_at = events.iter().position(
            |event| matches!(&event.event, S::Done { prompt_id: id } if id == &prompt_id),
        );
        let (Some(spent), Some(done)) = (spent_at, done_at) else {
            panic!("the budget-spent note and the done envelope both crossed: {events:?}");
        };
        assert!(
            spent < done,
            "the budget note crossed before the turn settled: {events:?}"
        );

        // The record: both thread rows landed, the run's metadata stamped
        // with the pipeline version and the finish reason done, and the
        // act's completed card folded into the rendered row.
        let chat_rows = store.chat_messages(&chat.id).expect("the chat reads");
        assert_eq!(
            chat_rows.len(),
            2,
            "the user side and the assistant side: {chat_rows:?}"
        );
        assert_eq!(chat_rows[0].role, "user");
        assert_eq!(chat_rows[0].parts[0]["text"].as_str(), Some(PROMPT));
        assert_eq!(chat_rows[1].role, "assistant");
        let metadata = chat_rows[1]
            .metadata
            .as_ref()
            .expect("the run metadata landed");
        assert_eq!(
            metadata["pipeline"].as_str(),
            Some(AGENT_PIPELINE_VERSION),
            "the run names its pipeline: {metadata}"
        );
        assert_eq!(
            metadata["finish_reason"].as_str(),
            Some("done"),
            "the turn landed done, the budget-truncated shape included: {metadata}"
        );
        assert_eq!(metadata["correlation"].as_str(), Some(CORR));
        assert!(
            metadata["tool_calls"]
                .as_u64()
                .is_some_and(|count| count >= 2),
            "at least a search and an act ran: {metadata}"
        );
        assert!(
            metadata["model"]
                .as_str()
                .is_some_and(|model| !model.is_empty()),
            "the run names its model: {metadata}"
        );
        assert!(
            chat_rows[1]
                .parts
                .as_array()
                .into_iter()
                .flatten()
                .any(|part| part["kind"] == "tool"
                    && part["name"] == act_ask.tool
                    && part["status"] == "ok"),
            "the act folded into the recorded row: {}",
            chat_rows[1].parts
        );

        // The audit trail, all under the turn's correlation id: the relay's
        // permission decision, the CLI's own row for the act verb it ran,
        // and the relay's audited tool outcome.
        let audits = testkit::audit_rows(&data_dir);
        let cli_verb = ACT_TOOLS
            .iter()
            .find(|(tool, _)| *tool == act_ask.tool)
            .map(|(_, verb)| *verb)
            .expect("the act verb maps to its CLI verb");
        assert!(
            audits.contains(&(
                CORR.to_owned(),
                format!("permission {}", act_ask.tool),
                "approved".to_owned(),
            )),
            "the approval decision is audited: {audits:?}"
        );
        assert!(
            audits.contains(&(CORR.to_owned(), cli_verb.to_owned(), "ok".to_owned())),
            "the act really executed its CLI verb, audited by the CLI itself: {audits:?}"
        );
        assert!(
            audits.contains(&(CORR.to_owned(), act_ask.tool.clone(), "ok".to_owned())),
            "the act's tool outcome is audited: {audits:?}"
        );
        let llm = testkit::llm_rows(&data_dir);
        assert!(
            llm.iter().any(|row| row.0 == CORR
                && row.2 == "chat"
                && row.3 == "assistant_chat"
                && row.4 > 0
                && row.5 > 0),
            "the turn's model calls ledger at the chat tier: {llm:?}"
        );

        // The cleanup: the seed and whatever the turn acted on are
        // withdrawn, the refresh settles the store and drains the act's
        // parked op against the withdrawn row, and the relay shuts down.
        delete_subject_mail(&engine, &cleanup, &account, &[SEED_SUBJECT]).await;
        refresh(&app).await;
        refresh(&app).await;
        let left = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .into_iter()
            .filter(|message| message.envelope.subject.as_deref() == Some(SEED_SUBJECT))
            .count();
        assert_eq!(left, 0, "the cleanup withdrew the seeded mail");
        relay.shutdown().await;
    });
}
