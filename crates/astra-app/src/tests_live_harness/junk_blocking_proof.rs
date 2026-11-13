// SPDX-License-Identifier: GPL-3.0-only
//! The gated junk-blocking proof against the live harness: the blocklist's
//! suppression leg over the real wire, and its release.
//!
//! The proof blocks the sending account's address through the settings
//! intent, delivers a note from it through the harness's own submission
//! path, and pins that the core's new-mail queue holds NO notice for it
//! while the note still lands in the recipient's INBOX (suppression only,
//! never auto-filing). It then unblocks the address, delivers a second note,
//! and pins the notice returns. Both copies leave through the shared sweep,
//! so the harness keeps its seeded shape.
//!
//! The gate is the deterministic new-mail lane, not the AI incoming pass, so
//! the proof needs no model endpoint: the offline battery is the model-free
//! proof, and this pins the wiring over a real delivery.

use astra_viewmodel::RowKey;
use engine_api::{AccountId, Provider};

use crate::protocol::SettingsIntent;
use crate::{reference::MessageRef, App, FolderRef, Intent};

use super::proof::{self, document};
use super::{harness, inbox_key, row_by_subject};

/// The subject the blocked delivery pins: unique among the harness fixtures.
const BLOCKED_SUBJECT: &str = "Harness junk blocking proof";

/// The subject the unblocked delivery pins: unique among the harness fixtures.
const UNBLOCKED_SUBJECT: &str = "Harness junk unblocking proof";

/// The words both notes carry; what the landed bodies open back.
const WORDS: &str = "The blocklist gate decides this delivery.";

/// The sending account's address, the sender the proof blocks and unblocks.
const SENDER_ADDRESS: &str = "astra1@example.test";

/// Submits one note from the sender to the recipient through the harness's
/// own submission path, queued and drained inside the dispatch.
async fn submit(app: &App<Box<dyn Provider>>, sender: &AccountId, subject: &str) {
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: subject.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );
}

/// The recipient's ordinary single-account pass and the queue drain the host
/// bridge runs after it: the same seam the delivery loop drives.
async fn land(app: &App<Box<dyn Provider>>, recipient: &AccountId) -> crate::NewMailBatch {
    app.sync_account(recipient).await;
    app.drain_new_mail()
}

/// Whether the batch holds a notice about `subject`.
fn holds(batch: &crate::NewMailBatch, subject: &str) -> bool {
    batch
        .messages
        .iter()
        .any(|notice| notice.subject.as_deref() == Some(subject))
}

/// The converging preamble: the first pass imports the harness state and
/// settles the queue's import suppression, the sweep withdraws a prior
/// interrupted run's copies of this proof's subjects, the second pass lands
/// the deletes, and the queue drains to a known empty.
async fn converge(app: &App<Box<dyn Provider>>, sender: &AccountId, recipient: &AccountId) {
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let subjects = [BLOCKED_SUBJECT, UNBLOCKED_SUBJECT];
    proof::delete_subject_mail(app, sender, &subjects).await;
    proof::delete_subject_mail(app, recipient, &subjects).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let settled = app.drain_new_mail();
    assert!(
        settled.messages.is_empty(),
        "the settled state holds no notices: {settled:?}"
    );
}

/// The withdrawal tail: both copies leave through the shared sweep, the next
/// passes land and observe the deletes, and neither account holds the mail.
async fn withdraw(app: &App<Box<dyn Provider>>, sender: &AccountId, recipient: &AccountId) {
    let subjects = [BLOCKED_SUBJECT, UNBLOCKED_SUBJECT];
    proof::delete_subject_mail(app, sender, &subjects).await;
    proof::delete_subject_mail(app, recipient, &subjects).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    for subject in subjects {
        assert!(
            app.mailbox_list()
                .rows
                .iter()
                .all(|row| row.subject != subject),
            "the proof's mail is withdrawn: {subject}"
        );
    }
}

/// The junk-blocking proof against the live harness: a blocked sender's
/// delivery raises no notice while the mail still lands in the INBOX, and
/// unblocking restores the notice, both copies withdrawn at the end.
#[tokio::test]
async fn a_blocked_senders_delivery_raises_no_notice_until_unblocked() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;
    converge(&app, &sender, &recipient).await;

    // Block the sending account's address through the settings intent; the
    // settings block answers the normalised entry.
    app.dispatch(Intent::Settings(SettingsIntent::BlockSender {
        address: SENDER_ADDRESS.to_owned(),
    }))
    .await
    .unwrap();
    assert!(
        app.settings()
            .settings
            .blocked_senders
            .contains(SENDER_ADDRESS),
        "the block rides the settings block"
    );

    // The blocked delivery: the queue holds no notice for it.
    submit(&app, &sender, BLOCKED_SUBJECT).await;
    let batch = land(&app, &recipient).await;
    assert_eq!(batch.overflow, 0, "one delivery, no overflow");
    assert!(
        !holds(&batch, BLOCKED_SUBJECT),
        "a blocked sender's delivery raises no notice: {batch:?}"
    );

    // Suppression only: the mail still landed in the recipient's INBOX.
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, BLOCKED_SUBJECT);
    assert_eq!(
        row.sender_address, SENDER_ADDRESS,
        "the blocked sender's mail still lands in the INBOX"
    );
    let RowKey::Message { account, key } = row.key.clone() else {
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
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the landed body is the composer's words: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // Unblock: the settings block answers an empty blocklist.
    app.dispatch(Intent::Settings(SettingsIntent::UnblockSender {
        address: SENDER_ADDRESS.to_owned(),
    }))
    .await
    .unwrap();
    assert!(
        app.settings().settings.blocked_senders.is_empty(),
        "the unblock empties the blocklist"
    );

    // The unblocked delivery: the notice returns.
    submit(&app, &sender, UNBLOCKED_SUBJECT).await;
    let batch = land(&app, &recipient).await;
    assert_eq!(batch.overflow, 0, "one delivery is one notice");
    assert!(
        holds(&batch, UNBLOCKED_SUBJECT),
        "the unblocked sender's delivery raises its notice: {batch:?}"
    );

    withdraw(&app, &sender, &recipient).await;
}
