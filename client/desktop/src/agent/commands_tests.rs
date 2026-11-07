// SPDX-License-Identifier: GPL-3.0-only
//! The agent command surface against the stub sidecar: the prompt round
//! trip with the correlation id generated when absent and echoed when
//! present, the permission answer, the abort, and the status pull. The
//! tests are plain sync tests that block on the state's own runtime (the
//! host's drive pattern): building a runtime inside a tokio test context
//! would panic when it drops.

use std::sync::Arc;

use super::{
    commands::{
        agent_abort_with, agent_prompt_with, agent_status_of, chat_delete_with, chat_list_with,
        chat_load_with, chat_rename_with, permission_response_with,
    },
    testkit::{audit_rows, llm_rows, scratch, stub_relay_with_store, CollectingSink},
    StreamPayload,
};
use crate::commands_fakes::{FakeApp, FakeStore};

/// A state over the stub sidecar in `mode`, with the sink the tests assert
/// against and the store the relay's ledgers and chat rows land in.
fn stub_state(name: &str, mode: &str) -> (crate::AppState, Arc<CollectingSink>) {
    let (relay, sink, _data, ai_store) = stub_relay_with_store(name, &[mode], &[], 3, 50);
    let app_state = crate::AppState {
        app: FakeApp::new(),
        agent: relay,
        mcp: crate::commands_fakes::host(&format!("{name}-agent")),
        plugins: Arc::new(crate::plugin::PluginRegistry::new(
            scratch(name).join("plugins"),
            vec![],
            true,
        )),
        llm: None,
        ai_store,
        connections: Arc::new(crate::AccountDials::empty(&scratch(name))),
        crypto_store: crate::commands_fakes::crypto_store(),
        photo_root: scratch(name).join("photos"),
        credential_store: FakeStore::accepting(),
        oauth: crate::oauth::wiring::offline_surface(),
        refusals: Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
        pending_mailto: std::sync::Mutex::new(None),
        pending_share: std::sync::Mutex::new(None),
        log_path: crate::logging::file_path(&scratch(name)),
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
    let receipt = app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "summarise today", None, None).await })
        .expect("the prompt is accepted");
    assert!(
        receipt.prompt_id.starts_with("p-"),
        "the command returns the prompt id: {:?}",
        receipt.prompt_id
    );
}

#[test]
fn a_prompt_with_a_correlation_carries_it_end_to_end() {
    let (app_state, sink) = stub_state("cmd-echo", "echo");
    app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(&app_state, "hello", Some("corr-42".to_owned()), None).await
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
        .block_on(|| async { agent_prompt_with(&app_state, "hello", None, None).await })
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
            agent_prompt_with(&app_state, "draft a reply", Some("corr-7".to_owned()), None).await
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
        .block_on(|| async {
            agent_prompt_with(&app_state, "count every message", None, None).await
        })
        .expect("the prompt is accepted")
        .prompt_id;

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
        .block_on(|| async { agent_prompt_with(&app_state, "hello", None, None).await })
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

#[test]
fn a_prompt_without_a_chat_mints_one_and_the_receipt_names_it() {
    // The hanging stub never answers, so the only row the chat can hold
    // is the one the dispatch itself recorded.
    let (app_state, _sink) = stub_state("cmd-mint", "hang");
    let receipt = app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(
                &app_state,
                "Summarise today's mail\nand flag the rest",
                None,
                None,
            )
            .await
        })
        .expect("the prompt is accepted");
    assert!(
        !receipt.chat_id.is_empty(),
        "the receipt names the minted chat"
    );

    let chats = chat_list_with(&app_state).expect("the list reads");
    assert_eq!(chats.len(), 1, "the minted chat stands in the list");
    assert_eq!(chats[0].id.as_str(), receipt.chat_id);
    assert_eq!(
        chats[0].title, "Summarise today's mail",
        "the title is the prompt's first line"
    );

    let loaded = chat_load_with(&app_state, &receipt.chat_id).expect("the load reads");
    assert_eq!(
        loaded.messages.len(),
        1,
        "the user row recorded at dispatch"
    );
    assert_eq!(loaded.messages[0].role, "user");
    assert_eq!(
        loaded.messages[0].parts,
        serde_json::json!([
            { "kind": "text", "text": "Summarise today's mail\nand flag the rest" }
        ]),
        "the user's parts are the prompt whole"
    );
}

#[test]
fn a_prompt_into_a_named_chat_records_both_sides_by_the_done_fold() {
    let (app_state, sink) = stub_state("cmd-thread", "echo");
    let receipt = app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(&app_state, "hello", Some("corr-9".to_owned()), None).await
        })
        .expect("the first prompt mints the chat");
    // The first turn settles before the second dispatches, so the row
    // order is the conversation's order, not two writers racing.
    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 3).await });
    let second = app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(&app_state, "and again", None, Some(receipt.chat_id.clone())).await
        })
        .expect("the second prompt rides the named chat");

    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 6).await });
    let loaded = chat_load_with(&app_state, &second.chat_id).expect("the load reads");
    assert_eq!(
        loaded.chat.id.as_str(),
        receipt.chat_id,
        "the named chat is the same thread"
    );
    assert_eq!(
        loaded.messages.len(),
        4,
        "two turns, both sides recorded: {:?}",
        loaded.messages
    );
    let roles: Vec<&str> = loaded.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "user", "assistant"]);
}

#[test]
fn a_prompt_with_an_unknown_chat_id_is_refused() {
    let (app_state, _sink) = stub_state("cmd-unknown-chat", "echo");
    let refused = app_state.runtime.block_on(|| async {
        agent_prompt_with(&app_state, "hello", None, Some("chat-nope".to_owned())).await
    });
    assert!(
        refused.is_err(),
        "an unknown chat id never records orphan rows"
    );
    assert!(
        chat_list_with(&app_state)
            .expect("the list reads")
            .is_empty(),
        "nothing was minted"
    );
}

#[test]
fn the_chat_commands_rename_and_delete_round_trip() {
    let (app_state, _sink) = stub_state("cmd-crud", "echo");
    let receipt = app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "plan my week", None, None).await })
        .expect("the prompt mints the chat");
    let id = receipt.chat_id;

    assert!(
        chat_rename_with(&app_state, &id, "Weekly planning").expect("the rename runs"),
        "a known chat renames"
    );
    let chats = chat_list_with(&app_state).expect("the list reads");
    assert_eq!(chats[0].title, "Weekly planning");
    let unknown_rename =
        chat_rename_with(&app_state, "chat-none", "Nothing").expect("the rename runs");
    assert!(!unknown_rename, "an unknown chat reports no row");

    assert!(
        chat_delete_with(&app_state, &id).expect("the delete runs"),
        "a known chat deletes"
    );
    assert!(
        chat_list_with(&app_state)
            .expect("the list reads")
            .is_empty(),
        "the thread list no longer holds the chat"
    );
    assert!(
        chat_load_with(&app_state, &id).is_err(),
        "a deleted chat refuses to load, so no stale thread paints"
    );
}

#[test]
fn a_second_prompt_while_one_is_live_answers_the_honest_error() {
    let (app_state, sink) = stub_state("cmd-one-live", "one-live");
    let _first = app_state
        .runtime
        .block_on(|| async {
            agent_prompt_with(&app_state, "count every message", None, None).await
        })
        .expect("the first prompt is accepted");
    app_state
        .runtime
        .block_on(|| async { await_events(&sink, 1).await });

    // The second prompt, into another chat: the sidecar's refusal crosses
    // as the prompt's own error, so the caller learns why nothing ran.
    let second = app_state
        .runtime
        .block_on(|| async { agent_prompt_with(&app_state, "another question", None, None).await })
        .expect("the envelope is written");
    let events = app_state
        .runtime
        .block_on(|| async { sink.await_events(2).await });
    match &events[1].event {
        StreamPayload::Error { message, prompt_id } => {
            assert_eq!(prompt_id.as_deref(), Some(second.prompt_id.as_str()));
            assert!(
                message.contains("already live"),
                "the refusal names the live turn: {message}"
            );
        }
        other => panic!("the refusal is an error payload, not {other:?}"),
    }
}
