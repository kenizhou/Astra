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
//! interrupted before its cleanup still converges. The reply and forward proofs
//! (their own files) share the fixtures this one established.
//!
//! The attachment proof extends the same wire: a send carrying one attachment,
//! the composer's document row with its bytes staged behind a minted handle
//! exactly as the submit resolves them, is delivered and read back through the
//! product's own reading pipeline, and the app-level byte read returns the
//! file name and the same bytes that were sent. The host dialog is not
//! live-testable and is offline-proven; this proof pins the read the dialog
//! drives.

use astra_composer::{AttachmentDisposition, AttachmentId, DraftAttachment, DraftBlobHandle};
use astra_viewmodel::{FolderRole, RowKey};

use crate::{reference::MessageRef, ComposerBlob, FolderRef, Intent};

use super::{
    harness, inbox_key,
    proof::{self, document, paragraph},
    row_by_subject,
};

/// The subject the send test pins: unique among the harness fixtures, so a row
/// carrying it is this test's mail and nothing the seed delivered.
const SUBJECT: &str = "Harness send proof";

/// The words the composer's document carries; what the reading pane opens back.
const WORDS: &str = "Sent from the composer through the harness submission port.";

/// The subject the attachment proof pins: unique among the harness fixtures.
const ATTACHMENT_SUBJECT: &str = "Harness attachment proof";

/// The words the attachment proof's document carries above its file row.
const ATTACHMENT_WORDS: &str = "One attachment rides this note.";

/// The file name the attachment proof stages and reads back.
const ATTACHMENT_FILE: &str = "proof-notes.txt";

/// The bytes the attachment proof stages, read back byte for byte. Printable,
/// so a failed comparison names its own difference.
const ATTACHMENT_BYTES: &[u8] = b"harness attachment proof bytes 0123456789";

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
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // Back to a known state: a run interrupted before its cleanup may have left the
    // proof mail behind, delivered or filed. The first pass presents what the store
    // holds, the sweep withdraws it, and the next two passes land the deletes and
    // observe them, so the send starts from an account that holds no proof mail.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the composer's gesture, queued before any server is asked and
    // drained inside the same dispatch, so the gesture ends with an answer.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
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
    let sent = FolderRef::from_parts(
        sender.as_str(),
        proof::role_key(&app, &sender, FolderRole::Sent),
    )
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
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
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

/// The document the attachment proof sends, beside the blob its row names:
/// the composer's own shape for one picked file, the bytes staged behind a
/// minted handle exactly as the submit resolves them.
fn document_with_attachment() -> (astra_composer::ComposerDocument, ComposerBlob) {
    let handle = DraftBlobHandle::new("harness-attachment-proof").expect("a non-empty handle");
    let document = astra_composer::ComposerDocument {
        blocks: vec![paragraph(ATTACHMENT_WORDS)],
        attachments: vec![DraftAttachment {
            id: AttachmentId::new("harness-attachment-proof").expect("a non-empty id"),
            blob: Some(handle.clone()),
            file_name: ATTACHMENT_FILE.to_owned(),
            media_type: "text/plain".to_owned(),
            size: Some(ATTACHMENT_BYTES.len() as u64),
            disposition: AttachmentDisposition::Attachment,
            data_url: None,
        }],
    };
    (
        document,
        ComposerBlob::new(handle, ATTACHMENT_BYTES.to_vec()),
    )
}

/// The attachment round trip against the live harness: a send carrying one
/// attachment is delivered, the reading snapshot lists the row, and the
/// app-level byte read returns the file name and the same bytes that were
/// sent, cleaned up after itself like every delivery proof.
#[tokio::test]
async fn an_attachment_round_trips_through_the_send_and_the_byte_read() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // The same converging sweep the send proof opens with, over this proof's
    // own subject.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[ATTACHMENT_SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[ATTACHMENT_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the document's attachment row with its bytes staged behind the
    // handle, resolved by the submit into the message that goes out.
    let (document, blob) = document_with_attachment();
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: ATTACHMENT_SUBJECT.to_owned(),
        document,
        blobs: vec![blob],
    })
    .await
    .unwrap();

    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );

    // The delivery: the recipient's INBOX holds the mail, the reading snapshot
    // lists the attachment row, and the byte read returns the same bytes.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, ATTACHMENT_SUBJECT);
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef {
            account: account.clone(),
            key: key.clone(),
        },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    let listed = reading
        .attachments
        .iter()
        .find(|row| row.file_name == ATTACHMENT_FILE)
        .unwrap_or_else(|| {
            panic!(
                "the reading snapshot lists the attachment row: {:?}",
                reading.attachments
            )
        });
    assert_eq!(
        listed.size,
        ATTACHMENT_BYTES.len() as u64,
        "the listed size is the size that was sent"
    );

    // The app-level save leg: the read the host's Save As drives, returning
    // the file name and the same bytes the composer staged.
    let content = app
        .attachment_content(&account, &key, listed.id)
        .await
        .expect("the attachment read")
        .expect("the listed row's part reads back");
    assert_eq!(
        content.file_name, ATTACHMENT_FILE,
        "the read names the file that was sent"
    );
    assert_eq!(
        content.bytes, ATTACHMENT_BYTES,
        "the read-back bytes are the bytes that were sent"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The cleanup: both copies withdrawn, the harness keeping its shape.
    proof::delete_subject_mail(&app, &sender, &[ATTACHMENT_SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[ATTACHMENT_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != ATTACHMENT_SUBJECT),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}
