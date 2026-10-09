// SPDX-License-Identifier: GPL-3.0-only
//! The gated reply test against the live harness: the whole reply chain over
//! the real wire, from the reading pane's Compose gesture to the threaded
//! delivery back at the original's sender.
//!
//! The proof needs a message astra2 holds to answer, and the seed leaves
//! astra2 nothing (its notes are astra1's), so the run makes one the proven
//! way first: astra1 submits a fresh note to astra2 through the engine's real
//! send path. astra2 then answers it through the product's own compose
//! pipeline: the Compose dispatch publishes the prefilled request (the
//! original's sender in `To`, the derived `Re:` subject, the original's
//! `Message-ID` as the threading header, the quoted original under its
//! attribution), the submit sends the composer's document above that quote as
//! a `SubmitRichReply`, and the delivery lands in astra1's INBOX carrying the
//! threading: the reply's `In-Reply-To` is the original's own `Message-ID`,
//! read back off the delivered copy's envelope through the product's own
//! sync, and its `References` chain holds the original as the conversation it
//! answered. The queue holds nothing once the gesture ends, and the run
//! leaves every account as it found it: the proof's copies (the fresh note
//! delivered and filed, the reply delivered and filed) are withdrawn through
//! the engine's own delete, so astra1's INBOX is the three seeded notes again
//! and astra2 holds nothing.

use astra_composer::{Block, ComposerDocument};
use astra_viewmodel::{ComposeKind, FolderRole, RowKey};
use engine_api::MessageIdHeader;

use crate::{reference::MessageRef, ComposeIntent, FolderRef, Intent};

use super::{
    harness, inbox_key,
    proof::{self, document, paragraph},
    row_by_subject,
};

/// The subject the fresh note carries: unique among the harness fixtures, so a
/// row carrying it is this test's mail and nothing the seed delivered.
const SUBJECT: &str = "Harness reply proof";

/// The subject the reply derives from it, the prefix the compose request pins.
const REPLY_SUBJECT: &str = "Re: Harness reply proof";

/// The words the fresh note carries; what the reply's quote holds.
const WORDS: &str = "The original words astra2 answers.";

/// The words the reply types above the quoted original.
const REPLY_WORDS: &str = "Answered from the composer through the harness submission port.";

/// The reply chain against the live harness: a compose request prefilled from
/// a message the store holds, a reply submitted under it, and a delivery that
/// threads under the original, cleaned up after itself.
#[tokio::test]
async fn the_reply_threads_under_the_original_and_cleans_up_after_itself() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // Back to a known state: a run interrupted before its cleanup may have left
    // any of the proof's copies behind. The first pass presents what the store
    // holds, the sweep withdraws them, and the next two passes land the deletes
    // and observe them.
    let proof_subjects = [SUBJECT, REPLY_SUBJECT];
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
    assert_eq!(
        row.sender_address, "astra1@example.test",
        "the original is delivered from the sending account"
    );

    // The original's own Message-ID off the delivered copy's envelope: the
    // header every later assertion measures the threading against.
    let original_id = app
        .engine
        .messages(&recipient)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.envelope.subject.as_deref() == Some(SUBJECT))
        .and_then(|message| message.envelope.message_id.into_iter().next())
        .expect("the delivered original carries a Message-ID");

    // The compose gesture: the reading pane's Reply on the delivered original,
    // answered by one published request the composer hydrates from.
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::Compose(ComposeIntent::Reply {
        message: MessageRef { account, key },
    }))
    .await
    .unwrap();
    let request = app
        .compose_request()
        .expect("the reply publishes its request");
    assert_eq!(request.kind, ComposeKind::Reply);
    assert_eq!(
        request.from, recipient,
        "the reply answers in the holder's account"
    );
    assert_eq!(
        request.to, "astra1@example.test",
        "the prefill answers the original's sender"
    );
    assert!(request.cc.is_empty(), "a plain reply leaves Cc to the user");
    assert_eq!(
        request.subject, REPLY_SUBJECT,
        "the subject takes the prefix"
    );
    assert_eq!(
        request.in_reply_to.as_deref(),
        Some(original_id.as_str()),
        "the threading header is the original's own Message-ID"
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
    assert!(
        quote.attribution.line.contains("wrote:"),
        "the attribution names the sender: {}",
        quote.attribution.line
    );
    assert!(
        quote
            .attribution
            .headers
            .iter()
            .any(|header| header.label == "From" && header.value.contains("astra1@example.test")),
        "the header block names the original's sender"
    );

    // The send: the composer's document, typed words above the retained quote,
    // submitted as a reply under the request's threading header.
    let mut blocks = vec![paragraph(REPLY_WORDS)];
    blocks.extend(request.document.blocks.iter().cloned());
    app.dispatch(Intent::SubmitRichReply {
        from: recipient.clone(),
        to: request.to.clone(),
        cc: request.cc.clone(),
        in_reply_to: MessageIdHeader::new(
            request
                .in_reply_to
                .as_deref()
                .expect("the request's header"),
        )
        .expect("the original's Message-ID"),
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
        "a delivered reply leaves nothing in the queue"
    );

    // The delivery: after one refresh the original's sender holds the answer,
    // and its envelope carries the threading the reply sent under.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, REPLY_SUBJECT);
    assert_eq!(
        row.sender_address,
        proof::RECIPIENT,
        "the delivered reply is from the answering account"
    );
    let delivered = app
        .engine
        .messages(&sender)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.envelope.subject.as_deref() == Some(REPLY_SUBJECT))
        .expect("the delivered reply in the store");
    assert_eq!(
        delivered.envelope.in_reply_to.first(),
        Some(&original_id),
        "the reply's In-Reply-To is the original's own Message-ID"
    );
    assert!(
        delivered
            .envelope
            .references
            .iter()
            .any(|header| header.as_str() == original_id.as_str()),
        "the References chain threads under the original: {:?}",
        delivered
            .envelope
            .references
            .iter()
            .map(|header| header.as_str().to_owned())
            .collect::<Vec<_>>()
    );

    // And the words are the words: the reading pane opens the answer through
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
    assert_eq!(reading.subject, REPLY_SUBJECT);
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(REPLY_WORDS) && body.contains(WORDS)),
        "the delivered body holds the typed words and the quoted original: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The answering account's half: the copy the engine filed in its Sent
    // folder after the submission.
    let sent = FolderRef::from_parts(
        recipient.as_str(),
        proof::role_key(&app, &recipient, FolderRole::Sent),
    )
    .expect("the answering account's Sent folder reference");
    app.dispatch(Intent::SelectFolder { folder: sent })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().any(|row| row.subject == REPLY_SUBJECT),
        "the filed copy sits in the answering account's Sent folder"
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
