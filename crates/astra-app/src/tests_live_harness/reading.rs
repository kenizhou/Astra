// SPDX-License-Identifier: GPL-3.0-only
//! The gated reading test against the live harness: the rich note the seed files
//! into Projects (a `multipart/related` html body referencing one remote image and
//! one inline `cid:` png, plus a notes.txt attachment) opened through the product's
//! own reading pipeline, over the same accepted-certificate dial the list tests
//! use.
//!
//! Each assertion is a contract the pipeline owns: the snapshot's html is the
//! sanitised body (the remote reference a `data-astra-src` placeholder, never a
//! live `src`), the inline `cid:` image arrived as a `data:image/png;base64` URI,
//! the gated reference is counted, the attachment list holds the notes.txt row,
//! and the open marked the message read through the same write path a list flip
//! drives, a state that survives a further refresh while the pane keeps showing
//! the body.

use std::sync::Arc;

use astra_viewmodel::RowKey;
use engine_api::{AccountId, EmailAddress, Engine, Provider};

use crate::{reference::MessageRef, Account, App, AppObserver, FolderRef, Intent};

use super::{dial_with_accepted_certificate, harness, mark_read, row_by_subject};

/// The Projects folder's key as the pane projects it for `account`, the reading
/// test's folder fixture (found by name: Projects carries no standard role).
fn projects_key(app: &App<Box<dyn Provider>>, account: &AccountId) -> String {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| row.folders.iter().find(|folder| folder.name == "Projects"))
        .map(|folder| folder.key.as_str().to_owned())
        .expect("the pane lists the account's Projects folder")
}

/// The reading pane against the live harness: the rich note opens through the real
/// dial, sync, and body pipeline, the sanitised body carries both image contracts
/// (remote gated, inline resolved), the attachment row is listed, and the open's
/// mark-read holds across a refresh that also republishes the body.
///
/// Like the list test, the run first puts the message back to unread through the
/// product's own write path: a warm volume may hold the read state an earlier run
/// left, and the open's flip is only provable from a known state.
#[tokio::test]
async fn the_reading_pane_opens_the_rich_note_and_the_read_state_survives_a_refresh() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account_id, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;

    let observer: Arc<dyn AppObserver> = Arc::new(super::CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers,
            identity: EmailAddress::new("astra1@example.test"),
        }],
        None,
        observer,
    );

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let projects = FolderRef::from_parts(account_id.as_str(), projects_key(&app, &account_id))
        .expect("the Projects folder reference");
    app.dispatch(Intent::SelectFolder { folder: projects })
        .await
        .unwrap();

    // Back to a known state: the rich note unread, the write reaching the server on
    // the same pass.
    mark_read(&app, "Harness rich note", false).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        row_by_subject(&snapshot, "Harness rich note").unread,
        "the rich note starts unread after the reset pass"
    );

    // The open: the row's own key is the message the pane opens.
    let key = row_by_subject(&snapshot, "Harness rich note").key.clone();
    let RowKey::Message { account, key } = key else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();

    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, "Harness rich note");
    assert_eq!(reading.from_name, "Astra Two");
    let html = reading.html.as_deref().expect("the sanitised html body");
    assert!(
        html.contains(r#"data-astra-src="https://astra-invalid.example/px.png""#),
        "the remote reference is gated behind the placeholder, html: {html}"
    );
    assert!(
        !html.contains("<img src=\"http"),
        "no live remote image source survives (a remote src would sit on the img \
         tag, past the placeholder), html: {html}"
    );
    assert!(
        html.contains("data:image/png;base64,"),
        "the inline cid: image arrived as a data URI, html: {html}"
    );
    assert!(
        html.contains("The rich harness note body."),
        "the sanitised body keeps the html text, html: {html}"
    );
    assert!(
        reading.remote_image_count >= 1,
        "the gated reference is counted: {}",
        reading.remote_image_count
    );
    assert!(
        reading
            .attachments
            .iter()
            .any(|row| row.file_name == "notes.txt"),
        "the attachment list holds notes.txt: {:?}",
        reading.attachments
    );

    // The open marked the message read, at once, in the list the row lives in.
    let snapshot = app.mailbox_list();
    assert!(
        !row_by_subject(&snapshot, "Harness rich note").unread,
        "the open flipped the row to read"
    );

    // Both halves survive a further refresh: the pane republishes the body, the read
    // state syncs back from the server.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let reading = app.reading().expect("the republished reading snapshot");
    assert!(
        reading
            .html
            .as_deref()
            .is_some_and(|html| html.contains("data:image/png;base64,")),
        "the pane still shows the body after the refresh"
    );
    let snapshot = app.mailbox_list();
    assert!(
        !row_by_subject(&snapshot, "Harness rich note").unread,
        "the read state survives a refresh"
    );
}
