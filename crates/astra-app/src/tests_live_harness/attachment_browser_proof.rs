// SPDX-License-Identifier: GPL-3.0-only
//! The gated attachment browser proof against the live harness: a submitted
//! note carrying one attachment (the send proof's round-trip fixture) is
//! delivered, the core's cross-message aggregation lists the part beside its
//! source reference, the app-level byte read returns the same bytes that were
//! sent, and both copies are withdrawn so the harness keeps its three-note
//! contract. The browse runs unfiltered over both accounts, so the row it
//! pins is the recipient's delivered copy, named by its own provider key.

use astra_composer::{AttachmentDisposition, AttachmentId, DraftAttachment, DraftBlobHandle};
use astra_viewmodel::{AttachmentBrowserFilter, RowKey};

use crate::{ComposerBlob, FolderRef, Intent};

use super::{
    harness, inbox_key,
    proof::{self, paragraph},
    row_by_subject,
};

/// The subject the browser proof pins: unique among the harness fixtures.
const SUBJECT: &str = "Harness attachment browser proof";

/// The words the proof's document carries above its file row.
const WORDS: &str = "One attachment rides this note for the browser.";

/// The file name the proof stages and reads back.
const FILE: &str = "browser-proof.txt";

/// The bytes the proof stages, read back byte for byte.
const BYTES: &[u8] = b"harness attachment browser proof bytes 0123456789";

/// The document the proof sends, beside the blob its row names: the composer's
/// own shape for one picked file, the bytes staged behind a minted handle
/// exactly as the submit resolves them.
fn document_with_attachment() -> (astra_composer::ComposerDocument, ComposerBlob) {
    let handle =
        DraftBlobHandle::new("harness-attachment-browser-proof").expect("a non-empty handle");
    let document = astra_composer::ComposerDocument {
        blocks: vec![paragraph(WORDS)],
        attachments: vec![DraftAttachment {
            id: AttachmentId::new("harness-attachment-browser-proof").expect("a non-empty id"),
            blob: Some(handle.clone()),
            file_name: FILE.to_owned(),
            media_type: "text/plain".to_owned(),
            size: Some(BYTES.len() as u64),
            disposition: AttachmentDisposition::Attachment,
            data_url: None,
        }],
    };
    (document, ComposerBlob::new(handle, BYTES.to_vec()))
}

/// The attachment browser against the live harness: a delivered attachment is
/// listed by the core's aggregation with its source reference, the byte read
/// returns the same bytes, and the run leaves both accounts as it found them.
#[tokio::test]
async fn the_browser_lists_a_delivered_attachment_and_the_byte_read_answers() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // The same converging sweep the send proof opens with, over this proof's
    // own subject.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
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
        subject: SUBJECT.to_owned(),
        document,
        blobs: vec![blob],
        crypto: crate::crypto::CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();

    // The delivery: the recipient's INBOX holds the note, its row keyed by the
    // provider key the browse's source reference must name.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, SUBJECT);
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };

    // The browse: the core's cross-message aggregation lists the delivered
    // part beside its source reference, unfiltered over both accounts.
    let view = app
        .attachment_browser(AttachmentBrowserFilter::default())
        .await;
    let entry = view
        .entries
        .iter()
        .find(|entry| entry.file_name == FILE && entry.source.key == key)
        .unwrap_or_else(|| {
            panic!(
                "the browser lists the delivered attachment: {:?}",
                view.entries
            )
        });
    assert_eq!(entry.source.account, account, "the row names the recipient");
    assert_eq!(entry.subject, SUBJECT, "the row carries the source subject");
    assert_eq!(
        entry.size,
        BYTES.len() as u64,
        "the listed size is the size sent"
    );
    assert!(
        view.coverage.failed_accounts.is_empty(),
        "both harness accounts scan: {:?}",
        view.coverage.failed_accounts
    );

    // The app-level save leg: the read the host's Save As drives, returning
    // the file name and the same bytes the composer staged.
    let content = app
        .attachment_content(&account, &key, entry.id)
        .await
        .expect("the attachment read")
        .expect("the listed row's part reads back");
    assert_eq!(
        content.file_name, FILE,
        "the read names the file that was sent"
    );
    assert_eq!(
        content.bytes, BYTES,
        "the read-back bytes are the bytes sent"
    );

    // The cleanup: both copies withdrawn, the harness keeping its shape.
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != SUBJECT),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}
