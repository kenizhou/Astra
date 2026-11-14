// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the watch-replies commands: the verbs thread the row reference
//! through to the core whole, the listing answers the core's rows, and a
//! malformed reference is refused before anything reaches the core.

use std::sync::Arc;

use astra_app::RowRef;
use serde_json::json;

use super::{watch_clear_of, watch_list_of, watch_set_of};
use crate::commands_fakes::{state, FakeApp, FakeStore};

#[test]
fn watch_set_hands_the_row_to_the_core() {
    let app = FakeApp::new();
    let state = state("watch-set-ok", Arc::clone(&app), FakeStore::accepting());

    let outcome = state
        .runtime
        .block_on(|| watch_set_of(&state, json!({"message": {"account": "work", "key": "m1"}})));

    assert!(outcome.unwrap(), "the core accepts the watch");
    let watched = &app.watches.lock().unwrap().watched;
    assert_eq!(watched.len(), 1, "one watch reaches the core");
    match &watched[0] {
        RowRef::Message(message) => {
            assert_eq!(message.account.as_str(), "work");
            assert_eq!(message.key.as_str(), "m1");
        }
        RowRef::Thread(_) => panic!("the row crossed as the message it named"),
    }
}

#[test]
fn watch_clear_hands_the_row_to_the_core() {
    let app = FakeApp::new();
    let state = state("watch-clear-ok", Arc::clone(&app), FakeStore::accepting());

    let outcome = state.runtime.block_on(|| {
        watch_clear_of(
            &state,
            json!({"thread": {"account": "work", "thread_id": "th-1"}}),
        )
    });

    assert!(outcome.unwrap(), "the core accepts the unwatch");
    let unwatched = &app.watches.lock().unwrap().unwatched;
    assert_eq!(unwatched.len(), 1, "one unwatch reaches the core");
    match &unwatched[0] {
        RowRef::Thread(thread) => {
            assert_eq!(thread.account.as_str(), "work");
            assert_eq!(thread.thread_id, "th-1");
        }
        RowRef::Message(_) => panic!("the row crossed as the conversation it named"),
    }
}

#[test]
fn watch_list_answers_the_cores_rows() {
    let app = FakeApp::new();
    let state = state("watch-list-ok", Arc::clone(&app), FakeStore::accepting());

    let outcome = state.runtime.block_on(|| watch_list_of(&state));

    assert!(outcome.unwrap().is_empty(), "the fake holds no watches");
    assert_eq!(
        app.watches.lock().unwrap().listed,
        1,
        "the listing was pulled"
    );
}

#[test]
fn a_malformed_row_is_refused_before_the_core() {
    let app = FakeApp::new();
    let state = state("watch-set-bad", Arc::clone(&app), FakeStore::accepting());

    let outcome = state.runtime.block_on(|| watch_set_of(&state, json!({})));

    assert!(outcome.is_err(), "an empty row is refused");
    assert!(
        app.watches.lock().unwrap().watched.is_empty(),
        "a refused reference reaches no core"
    );
}
