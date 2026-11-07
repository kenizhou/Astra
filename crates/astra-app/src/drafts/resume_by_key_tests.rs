// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the by-key draft resume: the Drafts row's named copy opens back
//! into a composer with its files pre-attached as removable rows in the
//! forward's own shape, the three refusal gates adopt nothing, the save after
//! a resume stamps the standing carried parts into the stored copy over the
//! same key and `Message-ID`, the plain submit resolves the standing rows'
//! bytes core-side, and the row's discard removes the named copy with the
//! composition discard's own queued discipline.

use std::sync::{Arc, Mutex};

use astra_composer::{
    AttachmentDisposition, Block, ComposerDocument, DraftAttachment, InlineContent, Paragraph,
    TextRun,
};
use engine_api::{
    AccountId, EmailAddress, Keyword, Message, MessageIdHeader, ProviderKey, SystemKeyword,
    UtcDateTime,
};

use crate::{
    dispatch_fakes as fakes, App, ComposeIntent, CompositionId, CryptoFlags, DraftsIntent, Intent,
    MessageRef,
};

/// A well-formed account id, the way these tests name one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A composition id, the way every drafts test names one.
fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).expect("a composition id")
}

/// The row's reference to a stored draft.
fn draft_ref(key: &str) -> MessageRef {
    MessageRef::from_parts("work", key.to_owned()).unwrap()
}

/// A stored draft whose source carries one pdf attachment, base64 `UERG`, the
/// three bytes `PDF`, the part the attachment reads list as id 0: the shape a
/// Drafts folder holds for a draft saved with a file.
fn attached_draft(key: &str, subject: &str, body: &str, minute: u8) -> (Message, Vec<u8>) {
    let mut message = fakes::mail(key, "drafts", subject);
    message
        .keywords
        .insert(Keyword::system(SystemKeyword::Draft));
    message.envelope.to = vec![EmailAddress::new("you@test.local")];
    message.envelope.message_id =
        vec![MessageIdHeader::new(format!("{key}@server.local")).unwrap()];
    message.received_at = Some(UtcDateTime::new(2026, 10, 9, 9, minute, 0).unwrap());
    let source = format!(
        "From: me@work.local\r\nTo: you@test.local\r\nSubject: {subject}\r\n\
         Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
         --mix\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n{body}\r\n\
         --mix\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\n\
         Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
         Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n\
         --mix--\r\n"
    );
    (message, source.into_bytes())
}

/// A one-paragraph document carrying `text`, with `rows` pre-attached.
fn document(text: &str, rows: Vec<DraftAttachment>) -> ComposerDocument {
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
        attachments: rows,
    }
}

/// A save intent for `id` whose document carries `rows`.
fn save(id: &str, text: &str, rows: Vec<DraftAttachment>) -> Intent {
    Intent::Drafts(DraftsIntent::Save {
        composition: composition(id),
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "The half".to_owned(),
        document: document(text, rows),
        then_close: false,
    })
}

/// A plain submit for `id` whose document carries `rows`.
fn submit(id: &str, text: &str, rows: Vec<DraftAttachment>) -> Intent {
    Intent::SubmitRichMail {
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "The half".to_owned(),
        document: document(text, rows),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: Some(composition(id)),
    }
}

/// An app over `provider`'s one `work` account, synced, recording surfaces.
async fn app_over(provider: fakes::FakeProvider) -> Arc<App<fakes::FakeProvider>> {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        vec![fakes::account("work", provider)],
        None,
        Arc::new(fakes::RecordingObserver {
            surfaces: Arc::clone(&surfaces),
        }),
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    Arc::new(app)
}

/// The provider holding an older draft and the newer attached draft `d-1`.
fn attached_provider() -> fakes::FakeProvider {
    let (older, older_source) = attached_draft("old-9", "An older half", "Older words", 1);
    let (newer, newer_source) = attached_draft("d-1", "The named half", "Named words", 2);
    fakes::FakeProvider::with_sources(
        vec![fakes::inbox(), fakes::folder("drafts", "Drafts")],
        vec![older, newer],
        vec![
            ("old-9".to_owned(), older_source),
            ("d-1".to_owned(), newer_source),
        ],
    )
}

#[tokio::test]
async fn the_named_draft_opens_with_its_parts_as_removable_rows() {
    let provider = attached_provider();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    assert_eq!(resumed.subject, "The named half");
    assert_eq!(resumed.to, "you@test.local");
    assert!(resumed.body_text.contains("Named words"));
    let [row] = resumed.attachments.as_slice() else {
        panic!(
            "the draft's own part stands as one removable row, not {:?}",
            resumed.attachments
        );
    };
    assert_eq!(row.file_name, "report.pdf");
    assert_eq!(row.media_type, "application/pdf");
    assert_eq!(row.size, Some(3));
    assert_eq!(row.disposition, AttachmentDisposition::Attachment);
    assert_eq!(
        row.blob
            .as_ref()
            .map(astra_composer::DraftBlobHandle::as_str),
        Some("carry:0"),
        "the row's handle names the stored copy's part"
    );
    assert_eq!(row.data_url, None);
}

#[tokio::test]
async fn the_first_save_after_the_resume_supersedes_the_named_copy() {
    let provider = attached_provider();
    let puts = provider.draft_puts();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    app.dispatch(save("compose-9", "edited", resumed.attachments.clone()))
        .await
        .unwrap();

    let puts = puts.lock().unwrap();
    assert_eq!(
        puts[0].1,
        Some(ProviderKey::new("d-1").unwrap()),
        "the first save after the resume supersedes the copy it opened, not the latest one"
    );
    assert_eq!(
        puts[0].0.message_id,
        MessageIdHeader::new("d-1@server.local").unwrap(),
        "the resumed composition saves under the stored copy's own Message-ID"
    );
}

#[tokio::test]
async fn the_save_stamps_the_standing_carried_parts_into_the_stored_copy() {
    let provider = attached_provider();
    let puts = provider.draft_puts();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    app.dispatch(save(
        "compose-9",
        "with the file",
        resumed.attachments.clone(),
    ))
    .await
    .unwrap();
    {
        let puts = puts.lock().unwrap();
        let [stamped] = puts[0].0.attachments.as_slice() else {
            panic!(
                "the standing carried part is stamped into the stored copy, not {:?}",
                puts[0].0.attachments
            );
        };
        assert_eq!(stamped.file_name, "report.pdf");
        assert_eq!(stamped.media_type, "application/pdf");
        assert_eq!(stamped.content, b"PDF");
    }

    // A row the composer removed is not stamped, so the stored copy the next
    // reopen reads keeps only the files still standing.
    app.dispatch(save("compose-9", "without the file", Vec::new()))
        .await
        .unwrap();
    let puts = puts.lock().unwrap();
    assert_eq!(puts[1].1, Some(ProviderKey::new("draft-1").unwrap()));
    assert!(
        puts[1].0.attachments.is_empty(),
        "a removed row's part does not ride the next save"
    );
}

#[tokio::test]
async fn the_send_resolves_the_standing_rows_bytes_core_side() {
    let provider = attached_provider();
    let submits = provider.submits();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    app.dispatch(submit(
        "compose-9",
        "on its way",
        resumed.attachments.clone(),
    ))
    .await
    .unwrap();

    let submits = submits.lock().unwrap();
    let sent = &submits[0];
    assert!(sent.text_body.contains("on its way"));
    let [part] = sent.attachments.as_slice() else {
        panic!(
            "the standing carried part rides the send, not {:?}",
            sent.attachments
        );
    };
    assert_eq!(part.file_name, "report.pdf");
    assert_eq!(part.content, b"PDF");
}

#[tokio::test]
async fn a_removed_row_does_not_ride_the_send() {
    let provider = attached_provider();
    let submits = provider.submits();
    let app = app_over(provider).await;

    app.resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    app.dispatch(submit("compose-9", "slimmed down", Vec::new()))
        .await
        .unwrap();

    let submits = submits.lock().unwrap();
    assert!(
        submits[0].attachments.is_empty(),
        "a row the composer removed carries nothing to the send"
    );
}

#[tokio::test]
async fn a_carry_handle_the_resume_never_parked_refuses_the_send() {
    let provider = attached_provider();
    let submits = provider.submits();
    let app = app_over(provider).await;

    app.resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    let stranger = vec![DraftAttachment {
        id: astra_composer::AttachmentId::new("carry-9").unwrap(),
        blob: Some(astra_composer::DraftBlobHandle::new("carry:9").unwrap()),
        file_name: "stranger.pdf".to_owned(),
        media_type: "application/pdf".to_owned(),
        size: Some(3),
        disposition: AttachmentDisposition::Attachment,
        data_url: None,
    }];
    app.dispatch(submit(
        "compose-9",
        "names a file it does not hold",
        stranger,
    ))
    .await
    .unwrap();

    assert!(
        submits.lock().unwrap().is_empty(),
        "a part that will not come back refuses the send: nothing queues"
    );
}

#[tokio::test]
async fn an_unresolvable_key_answers_none_and_adopts_nothing() {
    let provider = attached_provider();
    let puts = provider.draft_puts();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("gone-1"), composition("compose-9"))
        .await;
    assert_eq!(
        resumed, None,
        "a key the account does not hold resumes nothing"
    );

    app.dispatch(save("compose-9", "a fresh draft", Vec::new()))
        .await
        .unwrap();
    assert_eq!(
        puts.lock().unwrap()[0].1,
        None,
        "nothing was adopted, so the composition's first save supersedes nothing"
    );
}

#[tokio::test]
async fn a_key_that_is_not_a_draft_answers_none_and_adopts_nothing() {
    let delivered = fakes::delivered_at(fakes::mail("in-1", "inbox", "Not a draft"), 8, 0);
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox(), fakes::folder("drafts", "Drafts")],
        vec![delivered],
        vec![(
            "in-1".to_owned(),
            b"Content-Type: text/plain\r\n\r\nAn ordinary message".to_vec(),
        )],
    );
    let puts = provider.draft_puts();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("in-1"), composition("compose-9"))
        .await;
    assert_eq!(
        resumed, None,
        "a message that is not a draft resumes nothing"
    );

    app.dispatch(save("compose-9", "a fresh draft", Vec::new()))
        .await
        .unwrap();
    assert_eq!(
        puts.lock().unwrap()[0].1,
        None,
        "nothing was adopted, so the composition's first save supersedes nothing"
    );
}

#[tokio::test]
async fn a_draft_whose_body_will_not_read_answers_none_and_adopts_nothing() {
    let (unreadable, _) = attached_draft("mute-1", "The unreadable half", "Muted words", 3);
    // No source is registered for the key, so the body fetch refuses.
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox(), fakes::folder("drafts", "Drafts")],
        vec![unreadable],
        Vec::new(),
    );
    let puts = provider.draft_puts();
    let app = app_over(provider).await;

    let resumed = app
        .resume_draft_by_key(&draft_ref("mute-1"), composition("compose-9"))
        .await;
    assert_eq!(
        resumed, None,
        "a draft whose body will not read resumes nothing"
    );

    app.dispatch(save("compose-9", "a fresh draft", Vec::new()))
        .await
        .unwrap();
    assert_eq!(
        puts.lock().unwrap()[0].1,
        None,
        "nothing was adopted, so the composition's first save supersedes nothing"
    );
}

#[tokio::test]
async fn the_latest_entry_resume_carries_the_parts_too() {
    let provider = attached_provider();
    let app = app_over(provider).await;

    let resumed = app
        .resume_latest_draft(&account("work"), composition("compose-9"))
        .await
        .expect("the latest stored draft resumes");
    assert_eq!(resumed.subject, "The named half");
    assert_eq!(
        resumed.attachments.len(),
        1,
        "the latest-entry resume carries the same parts, so a transfer keeps them"
    );
}

// A forward standing beside a resumed draft resolves against its own original,
// never the draft record.
#[tokio::test]
async fn a_forward_submit_does_not_read_the_draft_record() {
    let provider = attached_provider();
    let submits = provider.submits();
    let app = app_over(provider).await;

    app.resume_draft_by_key(&draft_ref("d-1"), composition("compose-9"))
        .await
        .expect("the named draft resumes");
    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: draft_ref("d-1"),
        carry: vec![0],
    }))
    .await
    .unwrap();
    app.dispatch(Intent::SubmitRichForward {
        from: account("work"),
        to: "you@test.local".to_owned(),
        subject: "Fwd: The named half".to_owned(),
        document: document("forwarded", Vec::new()),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
    })
    .await
    .unwrap();

    assert_eq!(
        submits.lock().unwrap().len(),
        1,
        "the forward sends against its own original, unaffected by the draft record"
    );
}
