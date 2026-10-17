// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the message-list writes: the archive and trash moves (the
//! destination resolution, the thread fan-out, the honest no-target error)
//! and the typed keyword-write failures. The fixtures live in
//! `dispatch_fakes`; the keyword writes' own republish tests live in
//! `dispatch_list_tests`.

use std::sync::{Arc, Mutex};

use engine_api::{Mailbox, MailboxId, MailboxRole, ProviderKey};

use crate::{
    dispatch_fakes as fakes,
    reference::{MessageRef, RowRef, ThreadRef},
    Intent, MailWriteError, Surface,
};

use fakes::{account, app, delivered_at, folder, inbox, mail, threaded};

/// The role-bearing Archive folder.
fn archive() -> Mailbox {
    let mut mailbox = folder("archive", "Archive");
    mailbox.role = Some(MailboxRole::Archive);
    mailbox
}

/// The role-bearing All folder, the Gmail shape's archive home.
fn all_mail() -> Mailbox {
    let mut mailbox = folder("all", "All Mail");
    mailbox.role = Some(MailboxRole::All);
    mailbox
}

/// One inbox message under `key`, at a fixed instant.
fn inbox_mail(key: &str) -> engine_api::Message {
    delivered_at(mail(key, "inbox", "One"), 8, 0)
}

/// A message row on `account` under `key`.
fn message_row(account: &str, key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts(account, key.to_owned()).unwrap())
}

#[tokio::test]
async fn archiving_a_message_moves_it_to_the_accounts_archive_folder() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), archive()],
        vec![inbox_mail("m1"), inbox_mail("m2")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let signals = surfaces.lock().unwrap().len();

    app.dispatch(Intent::ArchiveMessage {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::move_to(
            ProviderKey::new("m1").unwrap(),
            MailboxId::try_from("archive").unwrap(),
        )],
        "one move edit through the provider, aimed at the account's archive"
    );
    assert!(
        surfaces.lock().unwrap().len() > signals,
        "the move write republishes the list, the same discipline the keyword \
         writes ride"
    );
}

#[tokio::test]
async fn trashing_a_message_moves_it_to_the_accounts_trash_folder() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox(), fakes::trash()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MoveToTrash {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::move_to(
            ProviderKey::new("m1").unwrap(),
            MailboxId::try_from("trash").unwrap(),
        )],
        "a trash move is a move edit, never a permanent delete"
    );
}

#[tokio::test]
async fn archiving_resolves_the_conventional_folder_name_when_no_role_is_tagged() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // Many IMAP servers tag Trash but not Archive: an untagged folder named
    // the conventional name is the archive.
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), folder("a1", "Archive")],
        vec![inbox_mail("m1")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::ArchiveMessage {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::move_to(
            ProviderKey::new("m1").unwrap(),
            MailboxId::try_from("a1").unwrap(),
        )],
        "the conventional name resolves the destination when no role is tagged"
    );
}

#[tokio::test]
async fn trashing_resolves_the_conventional_folder_name_when_no_role_is_tagged() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), folder("t1", "Deleted Items")],
        vec![inbox_mail("m1")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MoveToTrash {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::move_to(
            ProviderKey::new("m1").unwrap(),
            MailboxId::try_from("t1").unwrap(),
        )],
    );
}

#[tokio::test]
async fn archiving_falls_back_to_the_all_folder_when_no_archive_exists() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // Gmail has no Archive folder in any form: archiving is the absence of
    // the Inbox label, and the archived message's home is All Mail.
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox(), all_mail()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::ArchiveMessage {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_api::MailEdit::move_to(
            ProviderKey::new("m1").unwrap(),
            MailboxId::try_from("all").unwrap(),
        )],
        "the All folder is the archive of last resort"
    );
}

#[tokio::test]
async fn a_move_with_no_target_folder_is_an_error_not_a_silent_no_op() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![inbox_mail("m1")]);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let archive = app.archive_message(message_row("work", "m1")).await;
    let trash = app.move_to_trash(message_row("work", "m1")).await;

    assert_eq!(archive, Err(MailWriteError::NoTargetFolder));
    assert_eq!(trash, Err(MailWriteError::NoTargetFolder));
    assert!(
        edits.lock().unwrap().is_empty(),
        "no edit is issued without a destination to aim at"
    );
}

#[tokio::test]
async fn archiving_a_thread_row_moves_every_member() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), archive()],
        vec![
            threaded(inbox_mail("m1"), "th-1"),
            threaded(delivered_at(mail("m2", "inbox", "Two"), 8, 30), "th-1"),
            inbox_mail("m3"),
        ],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::ArchiveMessage {
        row: RowRef::Thread(ThreadRef::from_parts("work", "th-1".to_owned()).unwrap()),
    })
    .await
    .unwrap();

    let recorded = edits.lock().unwrap().clone();
    let mut destinations: Vec<(&str, &str)> = recorded
        .iter()
        .map(|edit| match edit {
            engine_api::MailEdit::MoveTo {
                target,
                destination,
            } => (target.as_str(), destination.key().as_str()),
            other => panic!("expected a move edit, got {other:?}"),
        })
        .collect();
    destinations.sort_unstable();
    assert_eq!(
        destinations,
        [("m1", "archive"), ("m2", "archive")],
        "every member of the conversation moves, the untouched row does not"
    );
}

#[tokio::test]
async fn a_write_on_an_unconfigured_account_reports_the_error() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox(), archive()], vec![inbox_mail("m1")]);
    let app = app(vec![account("work", provider)], &surfaces);

    let mark = app
        .mark_read(message_row("gone", "m1"), true)
        .await
        .unwrap_err();
    let archive = app
        .archive_message(message_row("gone", "m1"))
        .await
        .unwrap_err();

    assert_eq!(mark, MailWriteError::UnknownAccount);
    assert_eq!(archive, MailWriteError::UnknownAccount);
}

#[tokio::test]
async fn a_write_on_an_account_without_a_provider_reports_the_error() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox(), archive()], vec![inbox_mail("m1")]);
    let mut bare = account("bare", provider);
    bare.providers = Vec::new();
    let app = app(vec![bare], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let archive = app
        .archive_message(message_row("bare", "m1"))
        .await
        .unwrap_err();

    assert_eq!(archive, MailWriteError::NoProvider);
}

/// The write path signals the list surface, so a host watching it repaints;
/// the pin that the signal fires at all, beside the per-write tests above.
#[tokio::test]
async fn the_move_writes_signal_the_mailbox_list() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox(), fakes::trash()], vec![inbox_mail("m1")]);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    surfaces.lock().unwrap().clear();

    app.dispatch(Intent::MoveToTrash {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();
    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the trashed row's list is republished"
    );
}

/// A message row on `account` under `key`, the reference shape the
/// move-to-folder batch carries.
fn message_rows(account: &str, keys: &[&str]) -> Vec<MessageRef> {
    keys.iter()
        .map(|key| MessageRef::from_parts(account, (*key).to_owned()).unwrap())
        .collect()
}

#[tokio::test]
async fn moving_a_row_set_to_a_folder_loops_one_move_edit_per_row() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), folder("clients", "Clients")],
        vec![inbox_mail("m1"), inbox_mail("m2"), inbox_mail("m3")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    surfaces.lock().unwrap().clear();

    app.dispatch(Intent::MoveMessages {
        rows: message_rows("work", &["m1", "m2"]),
        folder: crate::FolderRef::from_parts("work", "clients".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [
            engine_api::MailEdit::move_to(
                ProviderKey::new("m1").unwrap(),
                MailboxId::try_from("clients").unwrap(),
            ),
            engine_api::MailEdit::move_to(
                ProviderKey::new("m2").unwrap(),
                MailboxId::try_from("clients").unwrap(),
            ),
        ],
        "one move edit per row, exactly as the single-row moves send them"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the moved rows' list is republished"
    );
}

#[tokio::test]
async fn a_move_to_the_folder_the_rows_already_sit_in_queues_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), folder("clients", "Clients")],
        vec![inbox_mail("m1")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.move_messages(
        message_rows("work", &["m1"]),
        crate::FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    )
    .await
    .unwrap();

    assert!(
        edits.lock().unwrap().is_empty(),
        "a row already in the destination is a no-op without an outbox op"
    );
}

#[tokio::test]
async fn a_move_to_folder_batch_refuses_cross_account_rows_typed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox(), folder("clients", "Clients")],
        vec![inbox_mail("m1")],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let outcome = app
        .move_messages(
            vec![
                MessageRef::from_parts("work", "m1".to_owned()).unwrap(),
                MessageRef::from_parts("home", "m9".to_owned()).unwrap(),
            ],
            crate::FolderRef::from_parts("work", "clients".to_owned()).unwrap(),
        )
        .await;

    assert_eq!(outcome, Err(MailWriteError::CrossAccount));
    assert!(
        edits.lock().unwrap().is_empty(),
        "a refused batch queues nothing, not even its legal rows"
    );
}
