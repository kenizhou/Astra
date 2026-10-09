// SPDX-License-Identifier: GPL-3.0-only
//! The gated send test against the live harness: the submit path exercised against
//! real SMTP submission and real delivery, over the same accepted-certificate dial
//! the other gated tests use.
//!
//! The proof needs both sides of a delivery, so the one app holds both harness
//! accounts: astra1 sends and astra2 receives, each dialed through the product's
//! own certificate acceptance walk, and the pane's per-account folders are how the
//! test reads each side back. A second account in the same app is how the product
//! scopes accounts; no backdoor stands beside the dial.
//!
//! The run is the contract the send path owns: a SubmitRichMail carrying the
//! composer's document is queued before any server is asked, delivered through the
//! harness's SMTP submission listener (the same leg the seed's notes ride, from the
//! sender's side this time), and the sender's copy is filed in their Sent folder
//! (Stalwart provisions "Sent Items" with the Sent role). The queue holds nothing
//! once the gesture ends, astra2's INBOX holds the mail with its subject and sender
//! pinned, and the words open in the reading pane.
//!
//! The run cleans up after itself through the engine's own outbox-mediated delete:
//! the delivered copy leaves astra2's INBOX and the filed copy leaves astra1's Sent,
//! so a rerun starts from the seeded state again and astra1's INBOX keeps its
//! three-note contract untouched. The same sweep runs at the start, so a run
//! interrupted before its cleanup still converges.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::{FolderRole, RowKey};
use engine_api::{AccountId, EmailAddress, Engine, MailEdit, Provider};

use crate::{reference::MessageRef, Account, App, AppObserver, FolderRef, Intent};

use super::{dial_with_accepted_certificate, harness, inbox_key, row_by_subject};

/// The subject the send test pins: unique among the harness fixtures, so a row
/// carrying it is this test's mail and nothing the seed delivered.
const SUBJECT: &str = "Harness send proof";

/// The words the composer's document carries; what the reading pane opens back.
const WORDS: &str = "Sent from the composer through the harness submission port.";

/// The receiving account's address, the second harness user.
const RECIPIENT: &str = "astra2@example.test";

/// A one-paragraph document carrying `text`, the smallest document a send holds.
fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

/// A unique idempotency key for one cleanup delete, from the wall clock and a
/// process counter: unique per call, so deleting two copies never collapses into
/// one op.
fn cleanup_idempotency() -> String {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("harness-send-cleanup:{nanos}:{seq}")
}

/// The account's folder key carrying `role`, as the pane projects it; the send
/// test's fixture for the Sent folder, where the dial binds the folder the engine
/// files the sender's copy into.
fn role_key(app: &App<Box<dyn Provider>>, account: &AccountId, role: FolderRole) -> String {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| row.folders.iter().find(|folder| folder.role == Some(role)))
        .map_or_else(
            || panic!("the pane lists the account's {role:?} folder"),
            |folder| folder.key.as_str().to_owned(),
        )
}

/// Deletes every message carrying this test's subject from `account` through the
/// engine's own outbox-mediated delete, the write path a delete action will ride:
/// queued durably first, then applied to the server. A message is addressed by its
/// own key, so one folder's provider can withdraw another folder's copy.
async fn delete_proof_mail(app: &App<Box<dyn Provider>>, account: &AccountId) {
    let handle = app
        .account_handle(account)
        .await
        .expect("the account the test dialed");
    let provider = handle
        .providers
        .first()
        .expect("the account's provider to delete through");
    let targets: Vec<_> = app
        .engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(|message| message.envelope.subject.as_deref() == Some(SUBJECT))
        .map(|message| message.id.key().clone())
        .collect();
    for target in targets {
        app.engine
            .edit_mail(
                provider,
                account,
                &cleanup_idempotency(),
                &MailEdit::Delete { target },
            )
            .await
            .expect("the cleanup delete");
    }
}

/// The send path against the live harness: a submitted message is delivered to its
/// recipient and the sender's copy is filed, the queue holds nothing once the
/// gesture ends, the recipient reads the words back through the product's own
/// reading pipeline, and the run leaves both accounts as it found them.
#[tokio::test]
async fn the_send_path_delivers_the_pinned_mail_and_cleans_up_after_itself() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (sender, sender_providers) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let (recipient, recipient_providers) =
        dial_with_accepted_certificate(&host, &port, RECIPIENT, &password).await;

    let observer: Arc<dyn AppObserver> = Arc::new(super::CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: EmailAddress::new("astra1@example.test"),
            },
            Account {
                id: recipient.clone(),
                providers: recipient_providers,
                identity: EmailAddress::new(RECIPIENT),
            },
        ],
        None,
        observer,
    );

    // Back to a known state: a run interrupted before its cleanup may have left the
    // proof mail behind, delivered or filed. The first pass presents what the store
    // holds, the sweep withdraws it, and the next two passes land the deletes and
    // observe them, so the send starts from an account that holds no proof mail.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    delete_proof_mail(&app, &sender).await;
    delete_proof_mail(&app, &recipient).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the composer's gesture, queued before any server is asked and
    // drained inside the same dispatch, so the gesture ends with an answer.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
    })
    .await
    .unwrap();

    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );

    // The delivery: after one refresh the recipient's INBOX holds the mail, its
    // subject and sender the words that were sent.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, SUBJECT);
    assert_eq!(
        row.sender_address, "astra1@example.test",
        "the delivered mail is from the sending account"
    );
    assert!(
        row.unread,
        "a mail nobody opened reads unread in the recipient's list"
    );

    // And the words are the words: the reading pane opens the delivered body
    // through the product's own pipeline, plain text and all.
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
    assert_eq!(reading.subject, SUBJECT);
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the delivered body is the composer's words: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The sender's half: the copy the engine filed in the Sent folder after the
    // submission, the record a person checks their message really left.
    let sent = FolderRef::from_parts(sender.as_str(), role_key(&app, &sender, FolderRole::Sent))
        .expect("the sender's Sent folder reference");
    app.dispatch(Intent::SelectFolder { folder: sent })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let filed = row_by_subject(&snapshot, SUBJECT);
    assert_eq!(
        filed.sender_address, "astra1@example.test",
        "the filed copy is the sender's own mail"
    );

    // The cleanup: both copies withdrawn through the same delete path, and the
    // next passes land and observe them, so a rerun starts clean and the harness
    // keeps its seeded shape.
    delete_proof_mail(&app, &sender).await;
    delete_proof_mail(&app, &recipient).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != SUBJECT),
        "the filed copy is withdrawn from the sender's Sent folder"
    );
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != SUBJECT),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}
