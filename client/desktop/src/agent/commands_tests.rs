// SPDX-License-Identifier: GPL-3.0-only
//! The agent command surface against the stub sidecar: the prompt round
//! trip with the correlation id generated when absent and echoed when
//! present, the permission answer, the abort, and the status pull. The
//! tests are plain sync tests that block on the state's own runtime (the
//! host's drive pattern): building a runtime inside a tokio test context
//! would panic when it drops.

use std::sync::Arc;

use super::{
    commands::{agent_abort_with, agent_prompt_with, agent_status_of, permission_response_with},
    testkit::{audit_rows, llm_rows, scratch, stub_relay, CollectingSink},
    StreamPayload,
};
use crate::commands_fakes::{FakeApp, FakeStore};

/// A state over the stub sidecar in `mode`, with the sink the tests assert
/// against.
fn stub_state(name: &str, mode: &str) -> (crate::AppState, Arc<CollectingSink>) {
    let (relay, sink, _data) = stub_relay(name, mode, 3, 50);
    let app_state = crate::AppState {
        app: FakeApp::new(),
        agent: relay,
        mcp: crate::commands_fakes::host(&format!("{name}-agent")),
        llm: None,
        connections: Arc::new(crate::AccountDials::empty(&scratch(name))),
        photo_root: scratch(name).join("photos"),
        credential_store: FakeStore::accepting(),
        oauth: crate::oauth::wiring::offline_surface(),
        refusals: Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
        pending_mailto: std::sync::Mutex::new(None),
        runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
    };
    (app_state, sink)
}

/// Waits for the sink to hold `count` stream events, up to five seconds.
async fn await_events(sink: &Arc<CollectingSink>, count: usize) {
    for _ in 0..1000 {
        if sink.agent_events.lock().unwrap().len() >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

#[test]
fn a_prompt_generates_the_correlation_and_echoes_it() {
    let (app_state, _sink) = stub_state("cmd-generate", "echo");
    let id = app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "summarise today", None).await })
        .expect("the prompt is accepted");
    assert!(
        id.starts_with("p-"),
        "the command returns the prompt id: {id}"
    );
}

#[test]
fn a_prompt_with_a_correlation_carries_it_end_to_end() {
    let (app_state, sink) = stub_state("cmd-echo", "echo");
    app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(&app_state, "hello", Some("corr-42".to_owned())).await
        })
        .expect("the prompt is accepted");

    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 3).await });
    let events = sink.agent_events.lock().unwrap();
    assert!(!events.is_empty(), "the turn streamed");
    for event in events.iter() {
        assert_eq!(
            event.correlation.as_deref(),
            Some("corr-42"),
            "every event of the turn echoes the caller's id"
        );
    }
}

#[test]
fn a_generated_correlation_reaches_the_ledger_row() {
    let (app_state, _sink) = stub_state("cmd-ledger", "echo");
    app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "hello", None).await })
        .expect("the prompt is accepted");

    let data = scratch("cmd-ledger");
    app_state.runtime.block_on(|| async {
        for _ in 0..1000 {
            if !llm_rows(&data).is_empty() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });
    let rows = llm_rows(&data);
    assert_eq!(rows.len(), 1, "one ledger row landed");
    assert!(
        !rows[0].0.is_empty(),
        "the generated correlation id reached the row"
    );
    assert_eq!(rows[0].3, "assistant_chat");
}

#[test]
fn a_permission_answer_round_trips() {
    let (app_state, sink) = stub_state("cmd-permission", "ask");
    app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(&app_state, "draft a reply", Some("corr-7".to_owned())).await
        })
        .expect("the prompt is accepted");

    let ask = app_state.runtime.block_on(|| async {
        for _ in 0..1000 {
            let id = sink
                .requests
                .lock()
                .unwrap()
                .first()
                .map(|ask| ask.id.clone());
            if let Some(id) = id {
                return id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("the ask never arrived")
    });

    app_state
        .runtime
        .block_on(|| async { permission_response_with(&app_state, &ask, true, false).await })
        .expect("the approval is delivered");

    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 3).await });
    let data = scratch("cmd-permission");
    assert!(
        audit_rows(&data)
            .iter()
            .any(|(_, verb, outcome)| verb == "permission save_draft" && outcome == "approved"),
        "the decision is audited"
    );

    let again = app_state
        .runtime
        .block_on(|| async { permission_response_with(&app_state, &ask, true, false).await });
    assert!(
        again.is_err(),
        "an answer is one-shot through the command too"
    );
}

#[test]
fn an_abort_settles_the_turn() {
    let (app_state, sink) = stub_state("cmd-abort", "hang");
    let id = app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "count every message", None).await })
        .expect("the prompt is accepted");

    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 1).await });
    app_state
        .runtime
        .block_on(|| async { agent_abort_with(&app_state, &id).await })
        .expect("the abort is accepted");

    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 2).await });
    let events = sink.agent_events.lock().unwrap();
    assert!(
        matches!(&events[1].event, StreamPayload::Done { prompt_id } if prompt_id == &id),
        "the abort settles the turn: {events:?}"
    );
}

#[test]
fn the_status_reports_the_state_word_and_model() {
    let (app_state, _sink) = stub_state("cmd-status", "echo");
    let before = agent_status_of(&app_state);
    assert_eq!(
        before.state, "down",
        "nothing is spawned before the first prompt"
    );
    assert_eq!(
        before.model, None,
        "no tier table stood behind the test state"
    );

    app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "hello", None).await })
        .expect("the prompt is accepted");
    let after = agent_status_of(&app_state);
    assert_eq!(after.state, "ready");
}

#[test]
fn a_state_over_the_shared_builder_spawns_nothing_until_asked() {
    // The shared command-test builder hands every suite a stub relay; a
    // suite that never prompts must never start a process for it.
    let app_state =
        crate::commands_fakes::state("cmd-idle", FakeApp::new(), FakeStore::accepting());
    assert_eq!(agent_status_of(&app_state).state, "down");
}
