// SPDX-License-Identifier: GPL-3.0-only
//! Core snooze tests over the fake provider: snooze records the stable
//! identity and moves the message into a created Snooze folder; the wake
//! re-resolves by Message-ID after the move synthesized a new key and moves
//! the message home, removing the entry; the key-reuse regression pins that
//! the stored provider key is NEVER the move-back target; the subject/sender
//! fallback resolves a copy whose Message-ID moved; and a conversation row
//! is refused.

use std::sync::{Arc, Mutex};

use engine_api::{Message, MessageIdHeader};

use crate::{
    dispatch_fakes as fakes,
    reference::{MessageRef, RowRef, ThreadRef},
    snooze::SnoozeError,
};

use fakes::{account, app, inbox, mail};

/// Sets the message's `Message-ID` header, the stable identity snooze keeps.
fn identified(mut message: Message, id: &str) -> Message {
    message.envelope.message_id = vec![MessageIdHeader::new(id).unwrap()];
    message
}

/// One inbox message under `key`, identified by `id`, about "One".
fn inbox_mail(key: &str, id: &str) -> Message {
    identified(mail(key, "inbox", "One"), id)
}

/// A message row on `account` under `key`.
fn message_row(key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts("work", key.to_owned()).unwrap())
}

/// An in-memory snooze store installed on the app.
fn installed(app: &crate::App<fakes::FakeProvider>) -> Arc<astra_snooze::SnoozeStore> {
    let store = Arc::new(astra_snooze::SnoozeStore::open_in_memory().unwrap());
    app.install_snooze_store(Arc::clone(&store));
    store
}

/// The move edits as (target, destination) pairs, in order.
fn moves(edits: &Arc<Mutex<Vec<engine_api::MailEdit>>>) -> Vec<(String, String)> {
    edits
        .lock()
        .unwrap()
        .iter()
        .map(|edit| match edit {
            engine_api::MailEdit::MoveTo {
                target,
                destination,
            } => (
                target.as_str().to_owned(),
                destination.key().as_str().to_owned(),
            ),
            other => panic!("expected a move edit, got {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn snoozing_records_the_identity_and_moves_into_a_created_snooze_folder() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1", "msg-1@example.test")])
            .with_tree_writes();
    let edits = Arc::clone(&provider.edits);
    let mailbox_edits = Arc::clone(&provider.mailbox_edits);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    app.snooze_message(message_row("m1"), 1_000).await.unwrap();

    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 1, "one snooze entry records");
    let entry = &entries[0];
    assert_eq!(entry.message_id.as_str(), "msg-1@example.test");
    assert_eq!(entry.provider_key.as_str(), "m1");
    assert_eq!(entry.origin_folder, "inbox");
    assert_eq!(entry.wake_at, 1_000);
    assert_eq!(entry.subject, "One");
    assert_eq!(entry.sender, "ada@example.test");
    assert!(
        mailbox_edits.lock().unwrap().iter().any(|edit| matches!(
            edit,
            engine_api::MailboxEdit::Create { name, .. } if name == "Snooze"
        )),
        "the Snooze folder is created (there is no Snooze role)"
    );
    assert_eq!(
        moves(&edits),
        [("m1".to_owned(), "Snooze".to_owned())],
        "the message moves out into the Snooze folder"
    );
}

#[tokio::test]
async fn the_wake_re_resolves_by_message_id_and_never_reuses_the_stored_key() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1", "msg-1@example.test")])
            .with_tree_writes();
    let edits = Arc::clone(&provider.edits);
    let messages = Arc::clone(&provider.messages);
    let updates = Arc::clone(&provider.updates);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    app.snooze_message(message_row("m1"), 1_000).await.unwrap();

    // The IMAP move synthesized a NEW key in the Snooze folder: the old key
    // m1 is gone, the copy lives under snooze-1 with the same Message-ID.
    let copy = identified(mail("snooze-1", "Snooze", "One"), "msg-1@example.test");
    *messages.lock().unwrap() = vec![copy.clone()];
    *updates.lock().unwrap() = vec![copy];
    edits.lock().unwrap().clear();

    let woken = app.wake_due_snoozes(2_000).await.unwrap();

    assert_eq!(woken, 1, "the due entry wakes");
    assert_eq!(
        moves(&edits),
        [("snooze-1".to_owned(), "inbox".to_owned())],
        "the move-back targets the re-resolved key, NEVER the stored m1"
    );
    assert!(store.list().unwrap().is_empty(), "the entry is removed");
}

#[tokio::test]
async fn the_wake_falls_back_to_subject_and_sender_when_the_message_id_moved() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1", "msg-1@example.test")])
            .with_tree_writes();
    let edits = Arc::clone(&provider.edits);
    let messages = Arc::clone(&provider.messages);
    let updates = Arc::clone(&provider.updates);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    app.snooze_message(message_row("m1"), 1_000).await.unwrap();

    // The copy carries a different Message-ID but the same subject/sender.
    let copy = identified(mail("snooze-1", "Snooze", "One"), "other@example.test");
    *messages.lock().unwrap() = vec![copy.clone()];
    *updates.lock().unwrap() = vec![copy];
    edits.lock().unwrap().clear();

    app.wake_due_snoozes(2_000).await.unwrap();

    assert_eq!(
        moves(&edits),
        [("snooze-1".to_owned(), "inbox".to_owned())],
        "the subject/sender fallback resolves the copy the Message-ID missed"
    );
    assert!(
        store.list().unwrap().is_empty(),
        "the fallback wake removes the entry too"
    );
}

#[tokio::test]
async fn a_conversation_row_is_refused() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1", "msg-1@example.test")])
            .with_tree_writes();
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    let outcome = app
        .snooze_message(
            RowRef::Thread(ThreadRef::from_parts("work", "th-1".to_owned()).unwrap()),
            1_000,
        )
        .await;

    assert!(matches!(outcome, Err(SnoozeError::ConversationRow)));
    assert!(
        store.list().unwrap().is_empty(),
        "a refused snooze records nothing"
    );
    assert!(
        edits.lock().unwrap().is_empty(),
        "a refused snooze queues nothing"
    );
}

#[tokio::test]
async fn a_wake_with_no_store_installed_answers_zero() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1", "msg-1@example.test")]);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    assert_eq!(app.wake_due_snoozes(2_000).await.unwrap(), 0);
}
