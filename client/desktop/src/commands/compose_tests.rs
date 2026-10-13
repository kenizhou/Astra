// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the compose command: the pull answers nothing while no request
//! is pending, then follows the family's dispatches through the prefills the
//! core computes, one publish per dispatch into the single slot. The fake
//! app and the state builder live in `commands_fakes`, one compiled copy for
//! every command test module.

use std::sync::Arc;

use astra_viewmodel::{ComposeKind, ComposeRequestSnapshot};

use super::compose_request_of;
use super::take_compose_request_of;
use crate::app_state::AppState;
use crate::commands_fakes::{compose_prefill, state, FakeApp, FakeStore};

/// One compose dispatch, as the webview sends it: the verb with the message
/// reference the reading pane acted on, byte for byte the JSON astra-app's
/// protocol tests pin for the family.
fn compose_json(verb: &str) -> serde_json::Value {
    serde_json::json!({"compose": {verb: {"message": {"account": "work", "key": "m1"}}}})
}

/// Waits until the compose pull answers `want`, then asserts it settled
/// there: the dispatches run on the runtime, so the test meets them where
/// they land.
fn wait_for_request(state: &AppState, want: Option<&ComposeRequestSnapshot>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while compose_request_of(state).as_ref() != want && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        compose_request_of(state),
        want.cloned(),
        "the pull settles on the app's published snapshot",
    );
}

#[test]
fn the_compose_pull_answers_none_while_no_request_is_pending() {
    let app = FakeApp::new();
    let state = state("compose-none", Arc::clone(&app), FakeStore::accepting());

    // Nothing has been composed: the pull answers `None`, never a panic, and
    // a webview holding a stale composer may clear it from the null alone.
    assert_eq!(compose_request_of(&state), None);
}

#[test]
fn the_compose_pull_follows_the_family_one_request_per_dispatch() {
    let app = FakeApp::new();
    let state = state("compose-pull", Arc::clone(&app), FakeStore::accepting());

    // A reply through the same JSON the webview sends; the pull settles on
    // the prefill the core computed for it, threading header and all.
    crate::commands::dispatch_intent(&state, compose_json("reply")).expect("the reply dispatches");
    let reply = compose_prefill(ComposeKind::Reply, "work");
    assert_eq!(reply.in_reply_to.as_deref(), Some("orig-1@server.test"));
    wait_for_request(&state, Some(&reply));

    // A forward the same way: the single slot holds the newest request only,
    // which names no recipient and threads under nothing.
    crate::commands::dispatch_intent(&state, compose_json("forward"))
        .expect("the forward dispatches");
    let forward = compose_prefill(ComposeKind::Forward, "work");
    assert_eq!(
        forward.to, "",
        "a forward's recipients are the user's to name"
    );
    assert!(
        forward.in_reply_to.is_none(),
        "a forward answers no message"
    );
    wait_for_request(&state, Some(&forward));
}

#[test]
fn the_reply_all_prefill_preserves_the_original_cc() {
    let app = FakeApp::new();
    let state = state(
        "compose-reply-all",
        Arc::clone(&app),
        FakeStore::accepting(),
    );

    crate::commands::dispatch_intent(&state, compose_json("reply_all"))
        .expect("the reply-all dispatches");
    let prefill = compose_prefill(ComposeKind::ReplyAll, "work");
    assert_eq!(
        prefill.cc, "carol@example.test",
        "the original's Cc stays Cc"
    );
    wait_for_request(&state, Some(&prefill));
}

#[test]
fn the_take_pull_hands_over_the_pending_request_and_consumes_it() {
    let app = FakeApp::new();
    let state = state("compose-take", Arc::clone(&app), FakeStore::accepting());

    crate::commands::dispatch_intent(&state, compose_json("reply")).expect("the reply dispatches");
    let reply = compose_prefill(ComposeKind::Reply, "work");
    wait_for_request(&state, Some(&reply));

    assert_eq!(
        take_compose_request_of(&state),
        Some(reply),
        "the take answers the pending request"
    );
    assert_eq!(
        compose_request_of(&state),
        None,
        "and the plain pull reads the consumed slot as empty"
    );
    assert_eq!(
        take_compose_request_of(&state),
        None,
        "a second take answers nothing"
    );
}

#[test]
fn the_take_pull_answers_none_while_no_request_is_pending() {
    let app = FakeApp::new();
    let state = state(
        "compose-take-none",
        Arc::clone(&app),
        FakeStore::accepting(),
    );

    assert_eq!(take_compose_request_of(&state), None);
}
