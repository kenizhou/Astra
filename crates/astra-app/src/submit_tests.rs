// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the threaded submit arms: the reply and the forward over the
//! same durable-outbox path the plain submit rides, with the threading each
//! carries read back off the draft the fake's SMTP leg records. The chain
//! rules pin mail-calendar's contract: a reply answers the original's own
//! `Message-ID` under its whole `References` chain, a forward carries the
//! chain and answers nothing, and the original is re-resolved at submit, so
//! what threads is the conversation the store holds, not a header the host
//! could have held stale.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::ComposeKind;
use engine_api::{AccountId, MessageIdHeader};

use crate::{
    dispatch_fakes as fakes, dispatch_fakes::ScriptedSend, App, ComposeIntent, Intent,
    OutboxIntent, QueuedRef, Surface,
};

/// A well-formed account id, the way the sending tests name one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// The original every fixture answers: Ada's report under her own message-id
/// and the chain it already threaded under, delivered at the fixture base
/// instant.
fn original() -> engine_api::Message {
    let mut message = fakes::mail("m1", "inbox", "Quarterly report");
    message.envelope.to = vec![engine_api::EmailAddress::new("me@work.local")];
    message.envelope.message_id = vec![header("orig-1@server.test")];
    message.envelope.references = ids(&["root@server.test", "mid@server.test"]);
    message
}

/// A threading header, the way the fixtures name one.
fn header(value: &str) -> MessageIdHeader {
    MessageIdHeader::new(value).unwrap()
}

/// A whole chain, the way the assertions expect one.
fn ids(values: &[&str]) -> Vec<MessageIdHeader> {
    values.iter().map(|value| header(value)).collect()
}

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

/// The reference to the original, the message every fixture answers.
fn m1() -> crate::MessageRef {
    crate::MessageRef::from_parts("work", "m1".to_owned()).unwrap()
}

/// The compose family's intent for `kind` over the original, so the core
/// holds the composition the submit answers.
fn compose(kind: ComposeKind) -> Intent {
    let message = m1();
    Intent::Compose(match kind {
        ComposeKind::Reply | ComposeKind::ReplyAll => ComposeIntent::Reply { message },
        ComposeKind::Forward => ComposeIntent::Forward {
            message,
            carry: Vec::new(),
        },
    })
}

/// The composer's Send button on a reply: a threaded submit about `subject`.
fn reply(subject: &str) -> Intent {
    Intent::SubmitRichReply {
        from: account("work"),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        in_reply_to: header("orig-1@server.test"),
        subject: subject.to_owned(),
        document: document("Agreed"),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    }
}

/// The composer's Send button on a forward: an unanchored submit about
/// `subject`.
fn forward(subject: &str) -> Intent {
    Intent::SubmitRichForward {
        from: account("work"),
        to: "bob@example.test".to_owned(),
        subject: subject.to_owned(),
        document: document("Look at this"),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    }
}

/// An in-memory app over one `work` account holding the original, synced so
/// the store resolves it, whose SMTP leg answers `script`.
async fn app_over(
    script: Vec<ScriptedSend>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<engine_api::Draft>>>) {
    let provider =
        fakes::FakeProvider::with_mail(vec![fakes::inbox()], vec![original()]).submitting(script);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, submits)
}

/// The one queued row the sending snapshot holds; panics when the queue is not
/// holding exactly one send, which is the assertion.
fn the_one_queued_send(app: &App<fakes::FakeProvider>) -> QueuedRef {
    let snapshot = app.sending();
    assert_eq!(snapshot.rows.len(), 1, "the queue holds exactly one send");
    let row = &snapshot.rows[0];
    QueuedRef::from_parts(row.queued.account.as_str(), row.queued.op).unwrap()
}

#[tokio::test]
async fn a_reply_submits_answering_the_original_under_its_whole_chain() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(vec![ScriptedSend::Delivered], &surfaces).await;
    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();

    app.dispatch(reply("Re: Quarterly report")).await.unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the reply went out");
    let draft = &submits.lock().unwrap()[0];
    assert_eq!(
        draft
            .in_reply_to
            .as_ref()
            .map(engine_api::MessageIdHeader::as_str),
        Some("orig-1@server.test")
    );
    assert_eq!(
        draft.references,
        ids(&["root@server.test", "mid@server.test", "orig-1@server.test"]),
        "the original's own chain followed by the original itself, oldest first"
    );
    assert_eq!(app.sending().rows.len(), 0, "delivered, so no row is left");
}

#[tokio::test]
async fn a_forward_submits_under_the_originals_chain_without_answering_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(vec![ScriptedSend::Delivered], &surfaces).await;
    app.dispatch(compose(ComposeKind::Forward)).await.unwrap();

    app.dispatch(forward("Fwd: Quarterly report"))
        .await
        .unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the forward went out");
    let draft = &submits.lock().unwrap()[0];
    assert!(
        draft.in_reply_to.is_none(),
        "a forward continues a conversation without answering a message"
    );
    assert_eq!(
        draft.references,
        ids(&["root@server.test", "mid@server.test", "orig-1@server.test"]),
        "the chain rides even though no reply pointer does"
    );
    assert_eq!(app.sending().rows.len(), 0);
}

#[tokio::test]
async fn a_queued_reply_goes_out_when_asked_with_its_threading_intact() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(
        vec![ScriptedSend::Unreachable, ScriptedSend::Delivered],
        &surfaces,
    )
    .await;
    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();

    app.dispatch(reply("Re: Quarterly report")).await.unwrap();
    let snapshot = app.sending();
    assert_eq!(snapshot.rows.len(), 1, "the reply is kept, not lost");
    assert_eq!(snapshot.rows[0].subject, "Re: Quarterly report");

    let queued = the_one_queued_send(&app);
    app.dispatch(Intent::Outbox(OutboxIntent::SendNow { queued }))
        .await
        .unwrap();

    assert_eq!(app.sending().rows.len(), 0, "the reply went out");
    let submits = submits.lock().unwrap();
    assert_eq!(
        submits.len(),
        2,
        "the parked reply was attempted again, not replaced"
    );
    for draft in submits.iter() {
        assert_eq!(
            draft.in_reply_to,
            Some(header("orig-1@server.test")),
            "the threading survives the queue's durable payload on every attempt"
        );
        assert_eq!(
            draft.references,
            ids(&["root@server.test", "mid@server.test", "orig-1@server.test"])
        );
    }
}

#[tokio::test]
async fn a_queued_forward_goes_out_when_asked_with_its_threading_intact() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(
        vec![ScriptedSend::Unreachable, ScriptedSend::Delivered],
        &surfaces,
    )
    .await;
    app.dispatch(compose(ComposeKind::Forward)).await.unwrap();

    app.dispatch(forward("Fwd: Quarterly report"))
        .await
        .unwrap();
    assert_eq!(app.sending().rows.len(), 1, "the forward is kept, not lost");

    let queued = the_one_queued_send(&app);
    app.dispatch(Intent::Outbox(OutboxIntent::SendNow { queued }))
        .await
        .unwrap();

    assert_eq!(app.sending().rows.len(), 0);
    let submits = submits.lock().unwrap();
    assert_eq!(submits.len(), 2);
    for draft in submits.iter() {
        assert!(draft.in_reply_to.is_none());
        assert_eq!(
            draft.references,
            ids(&["root@server.test", "mid@server.test", "orig-1@server.test"])
        );
    }
}

#[tokio::test]
async fn a_reply_without_a_pending_compose_request_threads_by_its_header_alone() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(vec![ScriptedSend::Delivered], &surfaces).await;
    // No compose dispatch: the reply a saved draft resumes as, whose original
    // the core holds no memory of, must still go out under its own header.
    app.dispatch(reply("Re: Quarterly report")).await.unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the reply went out");
    let draft = &submits.lock().unwrap()[0];
    assert_eq!(draft.in_reply_to, Some(header("orig-1@server.test")));
    assert_eq!(
        draft.references,
        ids(&["orig-1@server.test"]),
        "no original to read a chain from, so the parent alone is the chain"
    );
}

#[tokio::test]
async fn a_forward_without_a_resolvable_original_sends_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(Vec::new(), &surfaces).await;
    // No compose dispatch, so the core holds no original to thread under: the
    // forward's only source of its conversation is gone, and a forward that
    // threads under nothing is a new message pretending to be one.
    app.dispatch(forward("Fwd: Quarterly report"))
        .await
        .unwrap();

    assert_eq!(
        submits.lock().unwrap().len(),
        0,
        "no original, no forward: nothing was sent"
    );
    assert_eq!(app.sending().rows.len(), 0, "nothing was queued either");
}

#[tokio::test]
async fn a_reply_whose_header_disagrees_with_the_compose_source_threads_by_its_header() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over(vec![ScriptedSend::Delivered], &surfaces).await;
    // The compose request answered the stored original; the submit carries a
    // different header (a resumed draft of another message). The stored
    // original's chain does not belong to that answer, so only the header
    // threads: never another conversation's chain under a stale parent.
    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();
    let mut other = reply("Re: Something else");
    if let Intent::SubmitRichReply { in_reply_to, .. } = &mut other {
        *in_reply_to = header("other-9@server.test");
    }
    app.dispatch(other).await.unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1);
    let draft = &submits.lock().unwrap()[0];
    assert_eq!(draft.in_reply_to, Some(header("other-9@server.test")));
    assert_eq!(
        draft.references,
        ids(&["other-9@server.test"]),
        "a disagreeing source contributes no chain"
    );
}

/// The original's raw source beside the fixture original: a plain body and
/// one pdf attachment, base64 `UERG`, the three bytes `PDF`, the part the
/// reading snapshot lists as id 0 and a forward can carry.
fn attached_source() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(
        b"Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
          --mix\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
          The launch is ready.\r\n",
    );
    raw.extend_from_slice(
        b"--mix\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n",
    );
    raw.extend_from_slice(b"--mix--\r\n");
    raw
}

/// An app over one `work` account holding the attached original, synced, so
/// the compose arm, the store, and the part fetch all resolve.
async fn app_over_attached(
    script: Vec<ScriptedSend>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<engine_api::Draft>>>) {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![original()],
        vec![("m1".to_owned(), attached_source())],
    )
    .submitting(script);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, submits)
}

/// The forward's submit over `document`: the composer's Send button with the
/// document the request opened.
fn forward_over(subject: &str, document: ComposerDocument) -> Intent {
    Intent::SubmitRichForward {
        from: account("work"),
        to: "bob@example.test".to_owned(),
        subject: subject.to_owned(),
        document,
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    }
}

#[tokio::test]
async fn a_forward_carries_the_originals_standing_parts_as_attachments() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over_attached(vec![ScriptedSend::Delivered], &surfaces).await;
    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: m1(),
        carry: vec![0],
    }))
    .await
    .unwrap();
    let document = app
        .compose_request()
        .expect("the forward's prefill")
        .document;

    app.dispatch(forward_over("Fwd: Quarterly report", document))
        .await
        .unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the forward went out");
    let draft = &submits.lock().unwrap()[0];
    assert_eq!(
        draft.attachments,
        vec![engine_api::DraftAttachment::attachment(
            "report.pdf",
            "application/pdf",
            b"PDF".to_vec(),
        )],
        "the carried part rides as a regular file attachment, bytes and all"
    );
}

#[tokio::test]
async fn a_removed_carry_row_does_not_ride_the_send() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over_attached(vec![ScriptedSend::Delivered], &surfaces).await;
    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: m1(),
        carry: vec![0],
    }))
    .await
    .unwrap();
    let mut document = app
        .compose_request()
        .expect("the forward's prefill")
        .document;
    // The composer's remove control dropped the pre-attached row.
    document.attachments.clear();

    app.dispatch(forward_over("Fwd: Quarterly report", document))
        .await
        .unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the forward went out");
    let draft = &submits.lock().unwrap()[0];
    assert!(
        draft.attachments.is_empty(),
        "a removed row simply does not ride"
    );
}

#[tokio::test]
async fn a_carry_row_whose_part_will_not_resolve_refuses_the_send() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_over_attached(Vec::new(), &surfaces).await;
    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: m1(),
        carry: vec![9],
    }))
    .await
    .unwrap();
    // The document a client hands back names a carried part the original
    // does not hold: a send silently missing its file is worse than none.
    let mut document = document("Look at this");
    document.attachments.push(astra_composer::DraftAttachment {
        id: astra_composer::AttachmentId::new("carry-9").unwrap(),
        blob: Some(astra_composer::DraftBlobHandle::new("carry:9").unwrap()),
        file_name: "ghost.pdf".to_owned(),
        media_type: "application/pdf".to_owned(),
        size: Some(3),
        disposition: astra_composer::AttachmentDisposition::Attachment,
        data_url: None,
    });

    app.dispatch(forward_over("Fwd: Quarterly report", document))
        .await
        .unwrap();

    assert_eq!(
        submits.lock().unwrap().len(),
        0,
        "the carried row's bytes would not resolve, so nothing was sent"
    );
    assert_eq!(app.sending().rows.len(), 0, "nothing was queued either");
}
