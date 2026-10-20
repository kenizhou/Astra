// SPDX-License-Identifier: GPL-3.0-only
//! The gated forward test against the live harness: the forward chain over
//! the real wire, a conversation continued without answering a message.
//!
//! Like the reply proof, the run first makes the message it forwards the
//! proven way (astra1 submits a fresh note to astra2), then walks the
//! product's own pipeline over it: the Compose dispatch publishes the
//! prefilled request (no recipient, the derived `Fwd:` subject, no threading
//! header, the quoted original under a forwarded marker), the user names the
//! recipient the prefill left empty, and the submit sends the composer's
//! document above the quote as a `SubmitRichForward`. The delivery lands in
//! astra1's INBOX with the `Fwd:` subject and the forward's threading shape:
//! no `In-Reply-To` at all, and the `References` chain carrying the original
//! it continues. The queue holds nothing once the gesture ends, and the run
//! leaves every account as it found it, the same withdrawal the reply proof
//! makes.

use astra_composer::{Block, ComposerDocument};
use astra_viewmodel::{ComposeKind, FolderRole, RowKey};

use crate::{reference::MessageRef, ComposeIntent, FolderRef, Intent};

use super::{
    harness, inbox_key,
    proof::{self, document, paragraph},
    row_by_subject,
};

/// The subject the fresh note carries: unique among the harness fixtures.
const SUBJECT: &str = "Harness forward proof";

/// The subject the forward derives from it, the prefix the compose request pins.
const FORWARD_SUBJECT: &str = "Fwd: Harness forward proof";

/// The words the fresh note carries; what the forward's quote holds.
const WORDS: &str = "The original words astra2 forwards on.";

/// The words the forward types above the quoted original.
const FORWARD_WORDS: &str = "Forwarded from the composer through the harness submission port.";

/// The forward chain against the live harness: a compose request that names no
/// recipient, a forward submitted under the original's chain, and a delivery
/// that continues the conversation without answering it, cleaned up after
/// itself.
#[tokio::test]
async fn the_forward_carries_the_chain_without_answering_and_cleans_up_after_itself() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // Back to a known state: the same sweep the reply proof opens with, over
    // this proof's own subjects.
    let proof_subjects = [SUBJECT, FORWARD_SUBJECT];
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &proof_subjects).await;
    proof::delete_subject_mail(&app, &recipient, &proof_subjects).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The original, made the proven way: submitted from astra1, delivered to
    // astra2 through the harness's own submission port.
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

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, SUBJECT);

    // The original's own Message-ID off the delivered copy's envelope: the id
    // the forward's References chain is expected to carry.
    let original_id = app
        .engine
        .messages(&recipient)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.envelope.subject.as_deref() == Some(SUBJECT))
        .and_then(|message| message.envelope.message_id.into_iter().next())
        .expect("the delivered original carries a Message-ID");

    // The compose gesture: the reading pane's Forward on the delivered
    // original, answered by one published request that names no recipient.
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: MessageRef { account, key },
        carry: Vec::new(),
    }))
    .await
    .unwrap();
    let request = app
        .compose_request()
        .expect("the forward publishes its request");
    assert_eq!(request.kind, ComposeKind::Forward);
    assert_eq!(
        request.from, recipient,
        "the forward leaves the holder's account"
    );
    assert!(
        request.to.is_empty(),
        "a forward's recipients are the user's to name"
    );
    assert_eq!(
        request.subject, FORWARD_SUBJECT,
        "the subject takes the prefix"
    );
    assert!(
        request.in_reply_to.is_none(),
        "a forward continues a conversation without answering a message"
    );
    let quote = request
        .document
        .blocks
        .first()
        .and_then(|block| match block {
            Block::Quote(quote) => Some(quote),
            _ => None,
        })
        .expect("the document holds the quoted original");
    assert!(
        quote.body_plain.contains(WORDS),
        "the quote carries the original's words: {}",
        quote.body_plain
    );
    assert_eq!(
        quote.attribution.line, "Forwarded message",
        "the forward's attribution is the forwarded marker"
    );

    // The send: the user named the recipient the prefill left empty, and the
    // composer's document, typed words above the retained quote, goes out as a
    // forward.
    let mut blocks = vec![paragraph(FORWARD_WORDS)];
    blocks.extend(request.document.blocks.iter().cloned());
    app.dispatch(Intent::SubmitRichForward {
        from: recipient.clone(),
        to: "astra1@example.test".to_owned(),
        subject: request.subject.clone(),
        document: ComposerDocument {
            blocks,
            attachments: Vec::new(),
        },
        blobs: Vec::new(),
    })
    .await
    .unwrap();

    assert!(
        app.sending().rows.is_empty(),
        "a delivered forward leaves nothing in the queue"
    );

    // The delivery: after one refresh the named recipient holds the forward
    // under its prefixed subject, with the forward's threading shape on the
    // delivered copy's envelope.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, FORWARD_SUBJECT);
    assert_eq!(
        row.sender_address,
        proof::RECIPIENT,
        "the delivered forward is from the forwarding account"
    );
    let delivered = app
        .engine
        .messages(&sender)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.envelope.subject.as_deref() == Some(FORWARD_SUBJECT))
        .expect("the delivered forward in the store");
    assert!(
        delivered.envelope.in_reply_to.is_empty(),
        "a forward answers no message"
    );
    assert!(
        delivered
            .envelope
            .references
            .iter()
            .any(|header| header.as_str() == original_id.as_str()),
        "the References chain carries the original it continues: {:?}",
        delivered
            .envelope
            .references
            .iter()
            .map(|header| header.as_str().to_owned())
            .collect::<Vec<_>>()
    );

    // And the words are the words: the reading pane opens the forward through
    // the product's own pipeline, typed words and quoted original together.
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
    assert_eq!(reading.subject, FORWARD_SUBJECT);
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(FORWARD_WORDS) && body.contains(WORDS)),
        "the delivered body holds the typed words and the quoted original: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The forwarding account's half: the copy the engine filed in its Sent
    // folder after the submission.
    let sent = FolderRef::from_parts(
        recipient.as_str(),
        proof::role_key(&app, &recipient, FolderRole::Sent),
    )
    .expect("the forwarding account's Sent folder reference");
    app.dispatch(Intent::SelectFolder { folder: sent })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| row.subject == FORWARD_SUBJECT),
        "the filed copy sits in the forwarding account's Sent folder"
    );

    // The cleanup: every copy the proof made is withdrawn, and the harness
    // keeps its seeded shape, the three-note contract intact.
    proof::delete_subject_mail(&app, &sender, &proof_subjects).await;
    proof::delete_subject_mail(&app, &recipient, &proof_subjects).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        ["Harness note three", "Harness note two", "Harness note one"],
        "astra1's INBOX is the three seeded notes again, newest first",
    );
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    assert!(
        app.mailbox_list().rows.is_empty(),
        "astra2's INBOX holds nothing once the proof is withdrawn"
    );
}
