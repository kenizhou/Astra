// SPDX-License-Identifier: GPL-3.0-only
//! The relay's prompt and chat-thread paths against the stub sidecar: the
//! prompt round trip with its ledger row, the chat-naming prompt that
//! rides its own session and records both sides of its turn, and the turn
//! invariant's host leg (the sidecar's refusal of an overlapping prompt
//! crossing as the second prompt's own error). Split from
//! [`relay_tests`](super::relay_tests) for the file-size gate.

use super::{
    testkit::{audit_rows, llm_rows, stub_relay, stub_relay_with_store},
    StreamPayload,
};
use astra_ai::CorrelationId;

/// One test's runtime, built the way the host builds its own.
fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
}

/// One audit row's argument preview, read straight from the ledger file:
/// the shared helper keeps to the row's identity columns, so the preview
/// comes from the test's own connection.
fn recorded_preview(data: &std::path::Path, verb: &str) -> String {
    let conn =
        rusqlite::Connection::open(data.join("ai.sqlite")).expect("the ledger opens for reading");
    conn.query_row(
        "SELECT arg_preview FROM audit_log WHERE verb = ?1",
        [verb],
        |row| row.get(0),
    )
    .expect("the audit row reads back")
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
                    None,
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
fn a_prompt_naming_a_chat_rides_its_session_and_records_its_rows() {
    let runtime = runtime();
    let (relay, sink, _data, store) = stub_relay_with_store("chat-prompt", &["echo"], &[], 3, 50);
    let chat = store
        .insert_chat(&astra_ai::NewChat {
            id: astra_ai::ChatId::new("chat-1".to_owned()),
            title: "Thread one".to_owned(),
        })
        .expect("the chat lands");

    let id = runtime
        .block_on(|| async {
            relay
                .prompt(
                    "summarise the inbox",
                    Some(CorrelationId::new("corr-42".to_owned())),
                    Some(&chat.id),
                )
                .await
        })
        .expect("the prompt is sent");

    // The user's side records at dispatch, before the run answers.
    let dispatched = store.chat_messages(&chat.id).expect("the messages read");
    assert_eq!(dispatched.len(), 1, "the user row landed at dispatch");
    assert_eq!(dispatched[0].role, "user");

    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert_eq!(
        events.len(),
        3,
        "the stream holds exactly the turn: {events:?}"
    );
    for event in &events {
        assert_eq!(
            event.session_id, "agent:chat:chat-1",
            "the chat's prompts ride its own session"
        );
        assert_eq!(event.correlation.as_deref(), Some("corr-42"));
    }
    assert!(matches!(&events[2].event, StreamPayload::Done { prompt_id } if prompt_id == &id));

    // The done envelope is the fold point: the assistant's row lands with
    // the rendered parts and the run metadata before the done event
    // crosses to the UI.
    let messages = store.chat_messages(&chat.id).expect("the messages read");
    assert_eq!(
        messages.len(),
        2,
        "the assistant row landed at the fold: {messages:?}"
    );
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(
        messages[1].parts,
        serde_json::json!([{ "kind": "text", "text": "echo: summarise the inbox" }]),
        "the streamed text folded into the row's parts: {}",
        messages[1].parts
    );
    let metadata = messages[1]
        .metadata
        .as_ref()
        .expect("the run metadata landed");
    assert_eq!(metadata["model"], "stub-1");
    assert_eq!(metadata["tier"], "chat");
    assert_eq!(metadata["finish_reason"], "done");
    assert_eq!(metadata["tool_calls"], 0);
    assert_eq!(metadata["correlation"], "corr-42");
    // Every run stamps the assistant chat's pipeline version: the AI
    // rules' bump contract needs the stamp on the row a later diff reads.
    assert_eq!(
        metadata["pipeline"],
        super::relay::AGENT_PIPELINE_VERSION,
        "the run carries the pipeline version: {metadata}"
    );
}

#[test]
fn a_second_prompt_while_one_is_live_lands_the_refusal_for_it_alone() {
    let runtime = runtime();
    let (relay, sink, _data) = stub_relay("one-live", "one-live", 3, 50);
    let first = runtime
        .block_on(|| async { relay.prompt("count every message", None, None).await })
        .expect("the first prompt is sent");

    // The first prompt streams its delta, then hangs: one live turn.
    let events = runtime.block_on(|| async { sink.await_events(1).await });
    assert!(matches!(&events[0].event, StreamPayload::TextDelta { .. }));

    // The overlapping prompt from another chat's session: the sidecar's
    // refusal is an error envelope naming why, routed to the second
    // prompt's id alone; the first turn is not disturbed.
    let second = runtime
        .block_on(|| async {
            relay
                .prompt(
                    "another question",
                    Some(CorrelationId::new("corr-second".to_owned())),
                    Some(&astra_ai::ChatId::new("chat-b".to_owned())),
                )
                .await
        })
        .expect("the prompt envelope is written");
    let events = runtime.block_on(|| async { sink.await_events(2).await });
    match &events[1].event {
        StreamPayload::Error { message, prompt_id } => {
            assert_eq!(prompt_id.as_deref(), Some(second.as_str()));
            assert!(
                message.contains("already live"),
                "the refusal names the live turn: {message}"
            );
        }
        other => panic!("the second prompt's refusal is an error payload, not {other:?}"),
    }

    // The live turn still settles on its own abort: the refusal never
    // bounced it.
    runtime.block_on(|| async { relay.abort(&first).await });
    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert!(
        matches!(&events[2].event, StreamPayload::Done { prompt_id } if prompt_id == &first),
        "the first turn settles done: {events:?}"
    );
}

#[test]
fn an_ask_routes_to_the_turn_that_staged_its_call() {
    let runtime = runtime();
    let (relay, sink, data) = stub_relay("ask-routing", "hang", 3, 50);
    // The live turn rides its chat's own session and suspends mid-stream;
    // six more prompts wait in flight beside it, the window a refused
    // prompt sits pending in.
    runtime
        .block_on(|| async {
            relay
                .prompt(
                    "draft a reply",
                    Some(CorrelationId::new("corr-live".to_owned())),
                    Some(&astra_ai::ChatId::new("chat-a".to_owned())),
                )
                .await
        })
        .expect("the live prompt is sent");
    for at in 1..=6 {
        runtime
            .block_on(|| async {
                relay
                    .prompt(
                        &format!("meanwhile {at}"),
                        Some(CorrelationId::new(format!("corr-hang-{at}"))),
                        None,
                    )
                    .await
            })
            .expect("the overlapping prompt is sent");
    }
    let events = runtime.block_on(|| async { sink.await_events(7).await });
    assert_eq!(
        events.len(),
        7,
        "every prompt streams its delta: {events:?}"
    );

    // The live turn's tool call stages, then its ask crosses: both lines
    // replayed through the same door the read pump uses.
    let generation = runtime.block_on(|| async { relay.generation().await });
    runtime.block_on(|| async {
        relay
            .handle_line(
                r#"{"type":"event","session_id":"agent:chat:chat-a","event":{"kind":"tool_call","name":"save_draft","args":{"account":"acct-1"},"tool_call_id":"call-9"}}"#,
                generation,
            )
            .await;
    });
    runtime.block_on(|| async {
        relay
            .handle_line(
                r#"{"type":"permission_request","id":"ask-9","tool_call_id":"call-9","tool":"save_draft","level":"confirm","args":{"account":"acct-1"}}"#,
                generation,
            )
            .await;
    });

    // The ask carries the LIVE turn's correlation, never an arbitrary
    // in-flight prompt's.
    let ask = sink.requests.lock().unwrap()[0].clone();
    assert_eq!(
        ask.correlation.as_deref(),
        Some("corr-live"),
        "the ask attributes to the turn that staged the call"
    );

    // The decision's audit row carries the same attribution.
    runtime
        .block_on(|| async { relay.respond_permission(&ask.id, true, false).await })
        .expect("the answer is delivered");
    let audit = audit_rows(&data);
    assert!(
        audit
            .iter()
            .any(|(correlation, verb, _)| correlation == "corr-live"
                && verb == "permission save_draft"),
        "the decision audits under the live turn's correlation: {audit:?}"
    );
}

#[test]
fn a_tool_result_with_a_long_multibyte_output_truncates_safely() {
    let runtime = runtime();
    let (relay, _sink, data) = stub_relay("cjk-preview", "echo", 3, 50);
    // 300 CJK characters are 900 bytes, and byte 200 falls inside a
    // character (each spans three), so a byte-index truncate would panic
    // on the boundary before the cap is reached.
    let output = "语".repeat(300);
    let line = format!(
        r#"{{"type":"tool_result","tool_call_id":"call-cjk","tool":"search_messages","status":"ok","output":"{output}"}}"#
    );
    let generation = runtime.block_on(|| async { relay.generation().await });
    runtime.block_on(|| async { relay.handle_line(&line, generation).await });

    let rows = audit_rows(&data);
    assert_eq!(rows.len(), 1, "the audit row lands: {rows:?}");
    assert_eq!(rows[0].1, "search_messages");
    let preview = recorded_preview(&data, "search_messages");
    assert_eq!(
        preview.chars().count(),
        200,
        "the preview caps at characters, never cuts inside one"
    );
}
