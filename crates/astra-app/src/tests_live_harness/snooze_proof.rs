// SPDX-License-Identifier: GPL-3.0-only
//! The gated snooze proof against the live harness: a seeded note set aside
//! into a created Snooze folder (the core records the stable Message-ID and
//! moves it out), then woken by calling the wake body with a FUTURE now (no
//! real sleeping: the wake body takes now as a parameter), the note read back
//! in the INBOX after the wake re-resolved it by Message-ID over a real IMAP
//! move that synthesized a new key, the entry gone, and the Snooze folder
//! withdrawn so the harness keeps its three-note contract.

use std::sync::Arc;

use crate::{FolderIntent, FolderRef, Intent, RowRef};

use super::{
    folder_proof::{folder_row_by_name, note_ref, sender_app, withdraw_proof_folders},
    harness, inbox_key,
};

/// The snooze folder's name, the core's own conventional folder.
const SNOOZE: &str = "Snooze";

/// The snooze round trip against the live harness: the row menu's own verb
/// over the real dial, the wake forced with a future instant, the Message-ID
/// re-resolution proven over a move that minted a new provider key.
#[tokio::test]
async fn a_snooze_moves_a_note_out_and_the_wake_returns_it_by_message_id() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, account) = sender_app(&host, &port, &user, &password).await;
    let store = Arc::new(astra_snooze::SnoozeStore::open_in_memory().expect("the snooze store"));
    app.install_snooze_store(Arc::clone(&store));

    // The same converging sweep the folder proofs open with.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    withdraw_proof_folders(&app, &account).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The snooze: the note rides into the created Snooze folder, the stable
    // Message-ID recorded beside the diagnostic key.
    let note = note_ref(&app, &account, "Harness note two").await;
    app.snooze_message(RowRef::Message(note), 1_000)
        .await
        .expect("the snooze");
    assert_eq!(
        store.list().expect("the listing").len(),
        1,
        "the entry records"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snooze_folder = folder_row_by_name(&app, &account, SNOOZE);
    let snooze_ref = FolderRef::from_parts(account.as_str(), snooze_folder.key.as_str().to_owned())
        .expect("the Snooze folder reference");
    app.dispatch(Intent::SelectFolder {
        folder: snooze_ref.clone(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .any(|row| row.subject == "Harness note two"),
        "the snoozed note reads back in the Snooze folder on the next refresh"
    );

    // The wake, forced with a future now: the wake body syncs, re-resolves by
    // Message-ID (the IMAP move synthesized a new key), and moves the note
    // home. No real time passes.
    let woken = app.wake_due_snoozes(2_000).await.expect("the wake");
    assert_eq!(woken, 1, "the due entry wakes");
    assert!(
        store.list().expect("the listing").is_empty(),
        "the entry is gone"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The note is back in the INBOX, the harness's three-note contract intact.
    let inbox = FolderRef::from_parts(account.as_str(), inbox_key(&app, &account))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        ["Harness note three", "Harness note two", "Harness note one"],
        "the INBOX is the three seeded notes again, newest first"
    );

    // The withdrawal: the Snooze folder goes for good, the note already home.
    app.dispatch(Intent::Folders(FolderIntent::Delete {
        account: account.clone(),
        folder: snooze_folder.key.as_str().to_owned(),
    }))
    .await
    .unwrap();
    withdraw_proof_folders(&app, &account).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
