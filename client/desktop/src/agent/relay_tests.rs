// SPDX-License-Identifier: GPL-3.0-only
//! The relay against the stub sidecar script under a real Node process:
//! the init handshake, the prompt round trip with its ledger row, the
//! permission round trip with its audit rows, the abort, the command
//! surface, and the death drain. The tests block on their own runtime
//! (the host's drive pattern): this crate forbids `#[tokio::test]`'s
//! `block_on` by clippy config.

use std::sync::Arc;
use std::time::Duration;

use super::{
    protocol::{ModelApi, ModelConfig},
    supervisor,
    testkit::{audit_rows, configured_stub_relay, llm_rows, stub_relay, CollectingSink},
    StreamPayload,
};
use astra_ai::CorrelationId;

/// The pinned fixture lines the stale-generation test replays, from the
/// same shared files the protocol suite round-trips (four hops up from
/// this file to the repo root).
const LATE_PERMISSION_REQUEST: &str =
    include_str!("../../../../sidecar/src/fixtures/protocol/out-permission-request.json");
const LATE_LLM_CALL: &str =
    include_str!("../../../../sidecar/src/fixtures/protocol/out-llm-call.json");
const LIVE_EVENT: &str =
    include_str!("../../../../sidecar/src/fixtures/protocol/out-event-text-delta.json");

/// One test's runtime, built the way the host builds its own.
fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
}

/// Waits for the predicate over the sink, up to five seconds.
async fn until(predicate: impl Fn(&CollectingSink) -> bool, sink: &Arc<CollectingSink>) -> bool {
    for _ in 0..1000 {
        if predicate(sink) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

#[test]
fn the_init_handshake_reaches_ready() {
    let runtime = runtime();
    let (relay, sink, _data) = stub_relay("handshake", "echo", 3, 50);
    runtime
        .block_on(|| async { relay.ensure_started().await })
        .expect("the handshake completes");

    assert_eq!(relay.status().as_str(), "ready");
    let ready = sink.ready.lock().unwrap();
    assert_eq!(ready.len(), 1, "one ready emission");
    assert_eq!(ready[0].0, vec!["stub-1".to_string()]);
}

#[test]
fn a_prompt_streams_its_events_and_lands_its_ledger_row() {
    let runtime = runtime();
    let (relay, sink, data) = stub_relay("prompt", "echo", 3, 50);
    let id = runtime
        .block_on(|| async {
            relay
                .prompt(
                    "summarise the inbox",
                    Some(CorrelationId::new("corr-42".to_owned())),
                )
                .await
        })
        .expect("the prompt is sent");

    // state, text delta, done: the llm_call row bypasses the stream.
    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert_eq!(
        events.len(),
        3,
        "the stream holds exactly the turn: {events:?}"
    );
    for event in &events {
        assert_eq!(event.session_id, "agent:main");
        assert_eq!(event.correlation.as_deref(), Some("corr-42"));
    }
    assert!(matches!(&events[0].event, StreamPayload::State { state } if state == "thinking"));
    assert!(
        matches!(&events[1].event, StreamPayload::TextDelta { text } if text == "echo: summarise the inbox")
    );
    assert!(matches!(&events[2].event, StreamPayload::Done { prompt_id } if prompt_id == &id));

    let rows = llm_rows(&data);
    assert_eq!(rows.len(), 1, "one ledger row per sidecar call");
    assert_eq!(rows[0].0, "corr-42");
    assert_eq!(rows[0].1, "stub-1");
    assert_eq!(rows[0].2, "chat");
    assert_eq!(rows[0].3, "assistant_chat");
}

#[test]
fn a_prompt_without_a_correlation_generates_one() {
    let runtime = runtime();
    let (relay, sink, data) = stub_relay("generate", "echo", 3, 50);
    runtime
        .block_on(|| async { relay.prompt("hello", None).await })
        .expect("the prompt is sent");
    let events = runtime.block_on(|| async { sink.await_events(3).await });

    let correlation = events[0]
        .correlation
        .clone()
        .expect("a correlation was minted");
    assert!(!correlation.is_empty(), "the minted id is not blank");
    for event in &events[1..] {
        assert_eq!(event.correlation.as_deref(), Some(correlation.as_str()));
    }
    assert_eq!(
        llm_rows(&data)[0].0,
        correlation,
        "the ledger row carries it"
    );
}

#[test]
fn a_permission_ask_round_trips_through_the_bridge() {
    let runtime = runtime();
    let (relay, sink, data) = stub_relay("permission", "ask", 3, 50);
    runtime
        .block_on(|| async {
            relay
                .prompt(
                    "draft a reply",
                    Some(CorrelationId::new("corr-7".to_owned())),
                )
                .await
        })
        .expect("the prompt is sent");

    assert!(
        runtime.block_on(|| async {
            until(|sink| !sink.requests.lock().unwrap().is_empty(), &sink).await
        }),
        "the ask arrives"
    );
    let ask = sink.requests.lock().unwrap()[0].clone();
    assert_eq!(ask.tool, "save_draft");
    assert_eq!(ask.correlation.as_deref(), Some("corr-7"));
    assert_eq!(relay.pending_approvals().len(), 1);

    runtime
        .block_on(|| async { relay.respond_permission(&ask.id, true, false).await })
        .expect("the answer is delivered");

    // the approved tool result, the closing delta, and the done.
    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            StreamPayload::ToolResult {
                tool_call_id,
                status,
                ..
            } if tool_call_id == "stub-call-1" && status == "ok"
        )),
        "the approved tool result streams: {events:?}"
    );
    let resolved = sink.resolved.lock().unwrap();
    assert_eq!(resolved.len(), 1, "the settle is told to the UI");
    assert!(resolved[0].approved);

    let audit = audit_rows(&data);
    assert!(
        audit
            .iter()
            .any(|(_, verb, outcome)| verb == "permission save_draft" && outcome == "approved"),
        "the decision is audited: {audit:?}"
    );
    assert!(
        audit
            .iter()
            .any(|(_, verb, outcome)| verb == "save_draft" && outcome == "ok"),
        "the tool result is audited: {audit:?}"
    );
}

#[test]
fn a_refused_permission_settles_the_turn_without_the_tool() {
    let runtime = runtime();
    let (relay, sink, _data) = stub_relay("refuse", "ask", 3, 50);
    runtime
        .block_on(|| async {
            relay
                .prompt(
                    "draft a reply",
                    Some(CorrelationId::new("corr-8".to_owned())),
                )
                .await
        })
        .expect("the prompt is sent");
    assert!(
        runtime.block_on(|| async {
            until(|sink| !sink.requests.lock().unwrap().is_empty(), &sink).await
        }),
        "the ask arrives"
    );
    let ask = sink.requests.lock().unwrap()[0].id.clone();

    runtime
        .block_on(|| async { relay.respond_permission(&ask, false, false).await })
        .expect("the refusal is delivered");

    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert!(
        events.iter().any(
            |event| matches!(&event.event, StreamPayload::ToolResult { status, .. } if status == "rejected")
        ),
        "the refused tool result streams: {events:?}"
    );
}

#[test]
fn an_abort_settles_a_hanging_prompt() {
    let runtime = runtime();
    let (relay, sink, _data) = stub_relay("abort", "hang", 3, 50);
    let id = runtime
        .block_on(|| async { relay.prompt("count every message", None).await })
        .expect("the prompt is sent");

    let events = runtime.block_on(|| async { sink.await_events(1).await });
    assert!(
        matches!(&events[0].event, StreamPayload::TextDelta { .. }),
        "the hanging prompt streams first"
    );

    runtime.block_on(|| async { relay.abort(&id).await });
    let events = runtime.block_on(|| async { sink.await_events(2).await });
    assert!(
        matches!(&events[1].event, StreamPayload::Done { prompt_id } if prompt_id == &id),
        "the abort settles the turn: {events:?}"
    );
}

#[test]
fn a_command_round_trips_and_reports_the_scrubbed_env() {
    let runtime = runtime();
    let (relay, _sink, _data) = stub_relay("command", "echo", 3, 50);
    let value = runtime
        .block_on(|| async { relay.command("ping", serde_json::json!({})).await })
        .expect("the ping answers");

    let env = value
        .get("env")
        .and_then(|keys| keys.as_array())
        .expect("the ping reports the spawn environment");
    let names: Vec<&str> = env.iter().filter_map(|name| name.as_str()).collect();
    assert!(names.contains(&"PATH"), "PATH survives the scrub");
    assert!(
        names.contains(&"ASTRA_DATA_DIR"),
        "the data dir is injected: {names:?}"
    );
    assert!(
        names.iter().all(|name| !name.starts_with("ASTRA_LLM_")),
        "no ASTRA_LLM_* variable rides into the spawn: {names:?}"
    );

    let unknown = runtime.block_on(|| async { relay.command("nope", serde_json::json!({})).await });
    assert!(unknown.is_err(), "an unknown command answers an error");
}

#[test]
fn a_death_mid_prompt_drains_the_turn() {
    let runtime = runtime();
    let (relay, sink, _data) = stub_relay("drain", "die-mid", 5, 20);
    runtime
        .block_on(|| async {
            relay
                .prompt(
                    "keep talking",
                    Some(CorrelationId::new("corr-9".to_owned())),
                )
                .await
        })
        .expect("the prompt is sent");

    // One delta streams, then the stub exits: the supervisor drains the
    // turn as interrupted plus done, and the relay schedules a restart.
    assert!(
        runtime.block_on(|| async {
            until(
                |sink| {
                    sink.agent_events
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|event| matches!(&event.event, StreamPayload::Done { .. }))
                },
                &sink,
            )
            .await
        }),
        "the drained turn settles"
    );
    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert!(
        events.iter().any(
            |event| matches!(&event.event, StreamPayload::State { state } if state == "interrupted")
        ),
        "the drain says interrupted: {events:?}"
    );
    // The state read races the 20ms backoff: by the time the drain settles
    // the restart may already show "starting" or "ready". The attempts
    // counter is the race-free witness: the death path counts the attempt
    // before the backoff is even armed.
    assert!(
        runtime
            .block_on(|| async {
                for _ in 0..1000 {
                    if relay.attempts().await >= 1 {
                        return true;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                false
            }),
        "the budget has room, so a restart is scheduled"
    );
}

#[test]
fn a_restart_returns_to_ready_and_answers_prompts_again() {
    let runtime = runtime();
    let (relay, sink, _data) = stub_relay("restart", "echo", 3, 50);
    runtime
        .block_on(|| async { relay.ensure_started().await })
        .expect("the first start");

    runtime
        .block_on(|| async { relay.restart().await })
        .expect("the restart re-handshakes");
    assert_eq!(relay.status().as_str(), "ready");
    assert_eq!(
        sink.ready.lock().unwrap().len(),
        2,
        "a second ready emission for the second process"
    );

    runtime
        .block_on(|| async {
            relay
                .prompt("again", Some(CorrelationId::new("corr-10".to_owned())))
                .await
        })
        .expect("the prompt is sent after the restart");
    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert!(
        events.iter().any(
            |event| matches!(&event.event, StreamPayload::TextDelta { text } if text == "echo: again")
        ),
        "the new process answers: {events:?}"
    );
}

#[test]
fn a_configured_relay_delivers_its_model_configs_on_init() {
    let runtime = runtime();
    // The chat tier's endpoint, key included, rides the init line in band.
    let (relay, sink, _data) = configured_stub_relay(
        "init-configured",
        &[(
            "chat".to_string(),
            ModelConfig {
                api: ModelApi::AnthropicMessages,
                base_url: "https://api.anthropic.com".to_owned(),
                model: "claude-sonnet-4".to_owned(),
                api_key: Some("sk-ant-test".to_owned()),
            },
        )],
    );
    runtime
        .block_on(|| async { relay.ensure_started().await })
        .expect("the handshake completes");

    let events = runtime.block_on(|| async { sink.await_events(1).await });
    let echoed: serde_json::Value = serde_json::from_str(match &events[0].event {
        StreamPayload::TextDelta { text } => text,
        other => panic!("the echo-init report is a text delta: {other:?}"),
    })
    .expect("the report is JSON");
    assert_eq!(
        echoed["chat"]["api"], "anthropic-messages",
        "the configured tier is delivered: {echoed}"
    );
    assert_eq!(echoed["chat"]["base_url"], "https://api.anthropic.com");
    assert_eq!(echoed["chat"]["model"], "claude-sonnet-4");
    assert_eq!(echoed["chat"]["api_key"], "sk-ant-test");
}

#[test]
fn an_unconfigured_relay_omits_the_model_configs_field() {
    let runtime = runtime();
    let (relay, sink, _data) = configured_stub_relay("init-empty", &[]);
    runtime
        .block_on(|| async { relay.ensure_started().await })
        .expect("the handshake completes");

    // The field is omitted entirely (the protocol's skip rule), not
    // delivered as an empty object: the stub reports null for an absent
    // field and "{}" for a present empty one.
    let events = runtime.block_on(|| async { sink.await_events(1).await });
    assert!(
        matches!(&events[0].event, StreamPayload::TextDelta { text } if text == "null"),
        "the unconfigured relay omits the field: {:?}",
        events[0].event
    );
}

#[test]
fn stale_generation_lines_are_dropped_everywhere() {
    let runtime = runtime();
    // A long backoff keeps the supervisor's scheduled restart out of the
    // test's way; the generation moves once, by hand, the way every death
    // and restart path moves it.
    let (relay, sink, data) = stub_relay("stale-lines", "echo", 5, 10_000);
    runtime
        .block_on(|| async { relay.ensure_started().await })
        .expect("the handshake completes");
    let dead_generation = runtime.block_on(|| async { relay.generation().await });
    runtime
        .block_on(|| async { supervisor::handle_connection_dead(&relay, dead_generation).await });

    // The dead connection's pump delivers its late lines anyway: a
    // permission ask and an llm_call from before the bump. Neither may
    // register, emit, or land a row against the next generation.
    runtime.block_on(|| async {
        relay
            .handle_line(LATE_PERMISSION_REQUEST, dead_generation)
            .await;
    });
    runtime.block_on(|| async { relay.handle_line(LATE_LLM_CALL, dead_generation).await });
    assert!(
        sink.requests.lock().unwrap().is_empty(),
        "a stale permission ask never registers"
    );
    assert!(
        llm_rows(&data).is_empty(),
        "a stale llm_call never lands a ledger row"
    );
    assert!(
        sink.agent_events.lock().unwrap().is_empty(),
        "a stale line never reaches the stream"
    );

    // The current generation still routes: the same envelope shapes
    // deliver normally once the generation matches.
    let live_generation = runtime.block_on(|| async { relay.generation().await });
    runtime.block_on(|| async { relay.handle_line(LIVE_EVENT, live_generation).await });
    let events = runtime.block_on(|| async { sink.await_events(1).await });
    assert!(
        matches!(&events[0].event, StreamPayload::TextDelta { text } if text == "Hello"),
        "the current generation's line streams: {events:?}"
    );
}

#[test]
fn an_unknown_permission_answer_fails_without_starting_a_sidecar() {
    let runtime = runtime();
    let (relay, _sink, _data) = stub_relay("respond-order", "echo", 3, 50);
    let outcome =
        runtime.block_on(|| async { relay.respond_permission("no-such-ask", true, false).await });
    assert!(outcome.is_err(), "the unknown id is the typed refusal");
    assert_eq!(
        relay.status().as_str(),
        "down",
        "no sidecar is spawned just to refuse an answer"
    );
}
