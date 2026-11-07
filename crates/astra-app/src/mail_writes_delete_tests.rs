// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the permanent delete: the engine's delete edit through the
//! outbox-mediated row path, the conversation fan-out, and the pins that
//! hold the verb honest (no trash destination is resolved or needed, and a
//! row the account cannot answer reports the typed error rather than
//! skipping silently).

use std::sync::{Arc, Mutex};

use engine_api::ProviderKey;

use crate::{
    dispatch_fakes as fakes,
    reference::{MessageRef, RowRef, ThreadRef},
    Intent, MailWriteError, Surface,
};

use fakes::{account, app, delivered_at, folder, inbox, mail, threaded};

/// One inbox message under `key`, at a fixed instant.
fn inbox_mail(key: &str) -> engine_api::Message {
    delivered_at(mail(key, "inbox", "One"), 8, 0)
}

/// A message row on `account` under `key`.
fn message_row(account: &str, key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts(account, key.to_owned()).unwrap())
}

#[tokio::test]
async fn deleting_a_message_permanently_removes_it_through_the_outbox() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    surfaces.lock().unwrap().clear();

    app.dispatch(Intent::DeleteMessage {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::delete(
            ProviderKey::new("m1").unwrap()
        )],
        "the permanent delete is the engine's delete edit, never a move to trash"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the deleted row's list is republished, the same discipline the other \
         writes ride"
    );
}

#[tokio::test]
async fn deleting_a_conversation_row_deletes_every_member() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox()],
        vec![
            threaded(inbox_mail("m1"), "th-1"),
            threaded(delivered_at(mail("m2", "inbox", "Two"), 8, 30), "th-1"),
            inbox_mail("m3"),
        ],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::DeleteMessage {
        row: RowRef::Thread(ThreadRef::from_parts("work", "th-1".to_owned()).unwrap()),
    })
    .await
    .unwrap();

    let recorded = edits.lock().unwrap().clone();
    let mut targets: Vec<&str> = recorded
        .iter()
        .map(|edit| match edit {
            engine_api::MailEdit::Delete { target } => target.as_str(),
            other => panic!("expected a delete edit, got {other:?}"),
        })
        .collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        ["m1", "m2"],
        "every member of the conversation is deleted, the untouched row is not"
    );
}

#[tokio::test]
async fn the_permanent_delete_needs_no_trash_folder() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // An account whose tree holds no trash at all: the recoverable move
    // refuses with NoTargetFolder here, so the permanent verb working is the
    // pin that it resolves no destination, unlike every move.
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), folder("clients", "Clients")],
        vec![inbox_mail("m1")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.delete_message(message_row("work", "m1")).await.unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::delete(
            ProviderKey::new("m1").unwrap()
        )],
        "the delete edit needs no destination to aim at"
    );
}

#[tokio::test]
async fn a_delete_on_an_unconfigured_account_reports_the_error() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let app = app(vec![account("work", provider)], &surfaces);

    let outcome = app.delete_message(message_row("gone", "m1")).await;

    assert_eq!(outcome, Err(MailWriteError::UnknownAccount));
}
