// SPDX-License-Identifier: GPL-3.0-only
//! Core reminder tests over the fake provider: remind records the provider
//! key, subject, and sender read from the engine's own copy; the REGRESSION
//! pins that neither the remind nor the wake emits a move or flag edit (the
//! message stays put, unread untouched); the wake answers a notice per due
//! entry and removes the rows; a wake with no store answers nothing; and a
//! conversation row is refused.

use std::sync::{Arc, Mutex};

use engine_api::Message;

use crate::{
    dispatch_fakes as fakes,
    reference::{MessageRef, RowRef, ThreadRef},
    reminders::ReminderError,
};

use fakes::{account, app, inbox, mail};

/// One inbox message under `key`, about "One", from Ada.
fn inbox_mail(key: &str) -> Message {
    mail(key, "inbox", "One")
}

/// A message row on `account` under `key`.
fn message_row(key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts("work", key.to_owned()).unwrap())
}

/// An in-memory reminder store installed on the app.
fn installed(app: &crate::App<fakes::FakeProvider>) -> Arc<astra_reminders::ReminderStore> {
    let store = Arc::new(astra_reminders::ReminderStore::open_in_memory().unwrap());
    app.install_reminder_store(Arc::clone(&store));
    store
}

#[tokio::test]
async fn reminding_records_the_key_subject_and_sender_from_the_engine_copy() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    app.remind_message(message_row("m1"), 1_000).await.unwrap();

    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 1, "one reminder entry records");
    let entry = &entries[0];
    assert_eq!(entry.account.as_str(), "work");
    assert_eq!(entry.provider_key.as_str(), "m1");
    assert_eq!(entry.wake_at, 1_000);
    assert_eq!(entry.subject, "One");
    assert_eq!(entry.sender, "ada@example.test");
}

#[tokio::test]
async fn a_reminder_never_moves_or_flags_the_message() {
    // The reference red line: a reminder owns no engine state. Neither the
    // set nor the wake may queue a move or a flag edit; the message stays
    // put and its unread state is untouched.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let mailbox_edits = Arc::clone(&provider.mailbox_edits);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    app.remind_message(message_row("m1"), 1_000).await.unwrap();
    assert!(
        edits.lock().unwrap().is_empty(),
        "the remind queues no move or flag edit"
    );

    let notices = app.wake_due_reminders(2_000).unwrap();
    assert_eq!(notices.len(), 1, "the due entry wakes");
    assert!(
        edits.lock().unwrap().is_empty(),
        "the wake queues no move or flag edit either"
    );
    assert!(
        mailbox_edits.lock().unwrap().is_empty(),
        "a reminder creates no folder (it never moves mail)"
    );
    assert!(store.list().unwrap().is_empty(), "the entry is removed");
}

#[tokio::test]
async fn the_wake_answers_a_notice_per_due_entry_and_removes_the_rows() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1"), inbox_mail("m2")]);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    app.remind_message(message_row("m1"), 1_000).await.unwrap();
    app.remind_message(message_row("m2"), 3_000).await.unwrap();

    // Only m1 is due at 2_000; m2 stays for a later wake.
    let notices = app.wake_due_reminders(2_000).unwrap();
    assert_eq!(notices.len(), 1, "only the due entry answers a notice");
    let notice = &notices[0];
    assert_eq!(notice.account.as_str(), "work");
    assert_eq!(notice.key.as_str(), "m1");
    assert_eq!(notice.subject.as_deref(), Some("One"));
    assert_eq!(
        notice.from.as_ref().map(|from| from.email.as_str()),
        Some("ada@example.test")
    );
    assert_eq!(notice.preview, None, "the wake captures no preview");

    let remaining = store.list().unwrap();
    assert_eq!(remaining.len(), 1, "the future entry stays");
    assert_eq!(remaining[0].provider_key.as_str(), "m2");
}

#[tokio::test]
async fn a_conversation_row_is_refused() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = installed(&app);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    let outcome = app
        .remind_message(
            RowRef::Thread(ThreadRef::from_parts("work", "th-1".to_owned()).unwrap()),
            1_000,
        )
        .await;

    assert!(matches!(outcome, Err(ReminderError::ConversationRow)));
    assert!(
        store.list().unwrap().is_empty(),
        "a refused reminder records nothing"
    );
    assert!(
        edits.lock().unwrap().is_empty(),
        "a refused reminder queues nothing"
    );
}

#[tokio::test]
async fn a_wake_with_no_store_installed_answers_no_notices() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    assert!(
        app.wake_due_reminders(2_000).unwrap().is_empty(),
        "no store, no notices"
    );
}

#[tokio::test]
async fn a_remind_with_no_store_installed_records_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();

    // No store installed: the remind is a no-op, never an error, and queues
    // nothing.
    app.remind_message(message_row("m1"), 1_000).await.unwrap();
    assert!(
        edits.lock().unwrap().is_empty(),
        "a storeless remind queues nothing"
    );
}
