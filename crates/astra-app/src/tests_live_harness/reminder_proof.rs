// SPDX-License-Identifier: GPL-3.0-only
//! The gated reminder proof against the live harness: a submitted note (the
//! Write Contract mints its stable Message-ID) reminded about at a wake
//! instant (the core records the provider key, subject, and sender; the
//! message never moves), then woken by calling the wake body with a FUTURE
//! now (no real sleeping: the wake body takes now as a parameter), the notice
//! carrying the note's subject and sender, the entry gone, and the note STILL
//! in the INBOX unmoved (a reminder owns no engine state), the note withdrawn
//! so the harness keeps its three-note contract.

use std::sync::Arc;

use crate::crypto::CryptoFlags;
use crate::{FolderRef, Intent, RowRef};

use super::{
    folder_proof::{note_ref, sender_app, withdraw_proof_folders},
    harness, inbox_key,
    proof::{self, document},
};

/// The submitted note's subject: unique among the harness fixtures.
const SUBJECT: &str = "Reminder proof note";

/// The words the submitted note carries.
const WORDS: &str = "Reminded about by the reminder proof, left in place.";

/// The reminder round trip against the live harness: the row menu's own verb
/// over the real dial, the wake forced with a future instant, the notice's
/// captured words proven, and the note left unmoved in the INBOX.
#[tokio::test]
async fn a_reminder_notifies_about_a_note_and_leaves_it_unmoved_in_the_inbox() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, account) = sender_app(&host, &port, &user, &password).await;
    let store =
        Arc::new(astra_reminders::ReminderStore::open_in_memory().expect("the reminder store"));
    app.install_reminder_store(Arc::clone(&store));

    // The same converging sweep the folder proofs open with.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    withdraw_proof_folders(&app, &account).await;
    proof::delete_subject_mail(&app, &account, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // A submitted note carries a stable Message-ID (the seeded notes do not):
    // the Write Contract mints one before the send.
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

    // The reminder: the note's provider key, subject, and sender recorded;
    // the message stays put (no move, no flag, no folder created).
    let note = note_ref(&app, &account, SUBJECT).await;
    app.remind_message(RowRef::Message(note), 1_000)
        .await
        .expect("the reminder");
    assert_eq!(
        store.list().expect("the listing").len(),
        1,
        "the entry records"
    );

    // The wake, forced with a future now: the wake body builds the notice from
    // the captured words and removes the entry. No real time passes, no sync,
    // no move.
    let notices = app.wake_due_reminders(2_000).expect("the wake");
    assert_eq!(notices.len(), 1, "the due entry answers a notice");
    let notice = &notices[0];
    assert_eq!(
        notice.subject.as_deref(),
        Some(SUBJECT),
        "the notice carries the note's subject"
    );
    assert_eq!(
        notice.from.as_ref().map(|from| from.email.as_str()),
        Some("astra1@example.test"),
        "the notice carries the note's sender"
    );
    assert!(
        store.list().expect("the listing").is_empty(),
        "the entry is gone"
    );

    // The note is STILL in the INBOX, unmoved: a reminder owns no engine
    // state, so the message never left its folder.
    app.dispatch(Intent::RefreshMail).await.unwrap();
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
        "the reminded note reads back in the INBOX, unmoved"
    );

    // The withdrawal: the submitted note goes for good, the harness's
    // three-note contract intact.
    proof::delete_subject_mail(&app, &account, &[SUBJECT]).await;
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
