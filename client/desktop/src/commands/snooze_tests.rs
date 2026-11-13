// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the snooze command: the verb threads the message reference and
//! the wake instant through to the core whole, and a malformed reference is
//! refused before anything reaches the core.

use std::sync::Arc;

use astra_app::RowRef;

use super::snooze_message_of;
use crate::commands_fakes::{state, FakeApp, FakeStore};

#[test]
fn snooze_hands_the_row_and_wake_instant_to_the_core() {
    let app = FakeApp::new();
    let state = state("snooze-ok", Arc::clone(&app), FakeStore::accepting());

    let outcome = state
        .runtime
        .block_on(|| snooze_message_of(&state, "work", "m1", 1_000));

    assert!(outcome.is_ok(), "the core accepts the snooze");
    let snoozed = app.snoozed.lock().unwrap();
    assert_eq!(snoozed.len(), 1, "one snooze reaches the core");
    let (row, wake_at) = &snoozed[0];
    assert_eq!(*wake_at, 1_000, "the wake instant crosses untouched");
    match row {
        RowRef::Message(message) => {
            assert_eq!(message.account.as_str(), "work");
            assert_eq!(message.key.as_str(), "m1");
        }
        RowRef::Thread(_) => panic!("the verb names a message, never a conversation"),
    }
}

#[test]
fn a_malformed_reference_is_refused_before_the_core() {
    let app = FakeApp::new();
    let state = state("snooze-bad", Arc::clone(&app), FakeStore::accepting());

    let outcome = state
        .runtime
        .block_on(|| snooze_message_of(&state, "", "m1", 1_000));

    assert!(outcome.is_err(), "an empty account is refused");
    assert!(
        app.snoozed.lock().unwrap().is_empty(),
        "a refused reference reaches no core"
    );
}
