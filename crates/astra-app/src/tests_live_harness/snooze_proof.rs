// SPDX-License-Identifier: GPL-3.0-only
//! The gated snooze proof against the live harness: a submitted note (the
//! Write Contract mints its stable Message-ID, which the raw seeded notes
//! lack) set aside into a created Snooze folder (the core records the
//! Message-ID and moves it out), then woken by calling the wake body with a
//! FUTURE now (no real sleeping: the wake body takes now as a parameter), the
//! note read back in the INBOX after the wake re-resolved it by Message-ID over
//! a real IMAP move that synthesized a new key, the entry gone, and the note
//! and Snooze folder withdrawn so the harness keeps its three-note contract.

use std::sync::Arc;

use crate::crypto::CryptoFlags;
use crate::{FolderIntent, FolderRef, Intent, RowRef};

use super::{
    folder_proof::{folder_row_by_name, note_ref, sender_app, withdraw_proof_folders},
    harness, inbox_key,
    proof::{self, document},
};

/// The snooze folder's name, the core's own conventional folder.
const SNOOZE: &str = "Snooze";

/// The submitted note's subject: unique among the harness fixtures.
const SUBJECT: &str = "Snooze proof note";

/// The words the submitted note carries.
const WORDS: &str = "Set aside by the snooze proof, returned by the wake.";

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
    proof::delete_subject_mail(&app, &account, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // A submitted note carries a stable Message-ID (the seeded notes do not):
    // the Write Contract mints one before the send, so the core can record it.
    app.dispatch(Intent::SubmitRichMail {
        from: account.clone(),
        to: "astra1@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The snooze: the note rides into the created Snooze folder, the stable
    // Message-ID recorded beside the diagnostic key.
    let note = note_ref(&app, &account, SUBJECT).await;
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
            .any(|row| row.subject == SUBJECT),
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

    // The note is back in the INBOX.
    let inbox = FolderRef::from_parts(account.as_str(), inbox_key(&app, &account))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .any(|row| row.subject == SUBJECT),
        "the woken note reads back in the INBOX"
    );

    // The withdrawal: the submitted note and the Snooze folder go for good,
    // the harness's three-note contract intact.
    proof::delete_subject_mail(&app, &account, &[SUBJECT]).await;
    app.dispatch(Intent::Folders(FolderIntent::Delete {
        account: account.clone(),
        folder: snooze_folder.key.as_str().to_owned(),
    }))
    .await
    .unwrap();
    withdraw_proof_folders(&app, &account).await;
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
}
