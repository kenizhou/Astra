// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the category commands: each verb threads its arguments through to
//! the core whole (the row reference and the category id included), and a
//! malformed row reference is refused before anything reaches the core.

use std::sync::Arc;

use astra_app::RowRef;
use astra_categories::CategoryId;
use serde_json::json;

use super::{
    categories_assign_of, categories_create_of, categories_delete_of, categories_list_of,
    categories_unassign_of, categories_update_of,
};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// A fake app and state for one command test.
fn fixture(name: &str) -> (Arc<FakeApp>, crate::AppState) {
    let app = FakeApp::new();
    let state = state(name, Arc::clone(&app), FakeStore::accepting());
    (app, state)
}

#[test]
fn create_hands_the_name_and_colour_to_the_core() {
    let (app, state) = fixture("cat-create");

    let outcome = state
        .runtime
        .block_on(|| categories_create_of(&state, "Work", "#FF0000"));

    assert!(outcome.is_ok(), "the core accepts the create");
    let created = app.categories.lock().unwrap();
    assert_eq!(
        created.created,
        vec![("Work".to_owned(), "#FF0000".to_owned())]
    );
}

#[test]
fn list_pulls_the_definitions() {
    let (app, state) = fixture("cat-list");

    let outcome = state.runtime.block_on(|| categories_list_of(&state));

    assert!(outcome.is_ok(), "the listing reads");
    assert_eq!(app.categories.lock().unwrap().listed, 1);
}

#[test]
fn update_hands_the_id_name_and_colour_to_the_core() {
    let (app, state) = fixture("cat-update");

    let outcome = state
        .runtime
        .block_on(|| categories_update_of(&state, "work", "Projects", "#00FF00"));

    assert!(outcome.is_ok(), "the core accepts the update");
    let updated = app.categories.lock().unwrap();
    assert_eq!(
        updated.updated,
        vec![(
            CategoryId::new("work".to_owned()),
            "Projects".to_owned(),
            "#00FF00".to_owned()
        )]
    );
}

#[test]
fn delete_hands_the_id_to_the_core() {
    let (app, state) = fixture("cat-delete");

    let outcome = state
        .runtime
        .block_on(|| categories_delete_of(&state, "work"));

    assert!(outcome.is_ok(), "the core accepts the delete");
    assert_eq!(
        app.categories.lock().unwrap().deleted,
        vec![CategoryId::new("work".to_owned())]
    );
}

#[test]
fn assign_hands_the_row_and_category_to_the_core() {
    let (app, state) = fixture("cat-assign");
    let row = json!({"message": {"account": "work", "key": "m1"}});

    let outcome = state
        .runtime
        .block_on(|| categories_assign_of(&state, row, "work"));

    assert!(outcome.is_ok(), "the core accepts the assign");
    let assigned = app.categories.lock().unwrap();
    assert_eq!(assigned.assigned.len(), 1, "one assign reaches the core");
    let (row, category) = &assigned.assigned[0];
    assert_eq!(*category, CategoryId::new("work".to_owned()));
    match row {
        RowRef::Message(message) => {
            assert_eq!(message.account.as_str(), "work");
            assert_eq!(message.key.as_str(), "m1");
        }
        RowRef::Thread(_) => panic!("the row named a message"),
    }
}

#[test]
fn assign_accepts_a_thread_row() {
    let (app, state) = fixture("cat-assign-thread");
    let row = json!({"thread": {"account": "work", "thread_id": "th-1"}});

    let outcome = state
        .runtime
        .block_on(|| categories_assign_of(&state, row, "work"));

    assert!(outcome.is_ok(), "the core accepts a thread row");
    let assigned = app.categories.lock().unwrap();
    match &assigned.assigned[0].0 {
        RowRef::Thread(thread) => {
            assert_eq!(thread.account.as_str(), "work");
            assert_eq!(thread.thread_id, "th-1");
        }
        RowRef::Message(_) => panic!("the row named a conversation"),
    }
}

#[test]
fn unassign_hands_the_row_and_category_to_the_core() {
    let (app, state) = fixture("cat-unassign");
    let row = json!({"message": {"account": "work", "key": "m1"}});

    let outcome = state
        .runtime
        .block_on(|| categories_unassign_of(&state, row, "work"));

    assert!(outcome.is_ok(), "the core accepts the unassign");
    assert_eq!(app.categories.lock().unwrap().unassigned.len(), 1);
}

#[test]
fn a_malformed_row_reference_is_refused_before_the_core() {
    let (app, state) = fixture("cat-assign-bad");
    // An empty account does not parse as an AccountId: the row is malformed.
    let row = json!({"message": {"account": "", "key": "m1"}});

    let outcome = state
        .runtime
        .block_on(|| categories_assign_of(&state, row, "work"));

    assert!(outcome.is_err(), "a malformed row is refused");
    assert!(
        app.categories.lock().unwrap().assigned.is_empty(),
        "a refused reference reaches no core"
    );
}
