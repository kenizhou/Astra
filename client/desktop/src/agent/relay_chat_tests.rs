// SPDX-License-Identifier: GPL-3.0-only
//! The relay's prompt and chat-thread paths against the stub sidecar: the
//! prompt round trip with its ledger row, the chat-naming prompt that
//! rides its own session and records both sides of its turn, and the turn
//! invariant's host leg (the sidecar's refusal of an overlapping prompt
//! crossing as the second prompt's own error). Split from
//! [`relay_tests`](super::relay_tests) for the file-size gate.

use super::{
    testkit::{llm_rows, stub_relay, stub_relay_with_store},
    StreamPayload,
};
use astra_ai::CorrelationId;

/// One test's runtime, built the way the host builds its own.
fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
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
