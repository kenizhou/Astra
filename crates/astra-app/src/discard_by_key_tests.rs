// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the Drafts row's own discard: the by-key removal of one stored
//! copy, its queued shape when the server refuses, and the parked-save
//! withdrawal that keeps a waiting save from storing a draft the user just
//! deleted. A child of the drafts module, split from the resume's by-key
//! tests so each file keeps one concern under the line cap.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use engine_api::{
    AccountId, EmailAddress, Keyword, Message, MessageIdHeader, PendingOpKind, ProviderKey,
    SystemKeyword, UtcDateTime,
};

use crate::{dispatch_fakes as fakes, App, CompositionId, DraftsIntent, Intent, MessageRef};

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

/// A stored draft whose source carries one pdf attachment, the same shape the
/// resume's by-key tests open.
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

/// A save intent whose document carries `text` and nothing attached.
fn save(id: &str, text: &str) -> Intent {
    Intent::Drafts(DraftsIntent::Save {
        composition: composition(id),
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "The half".to_owned(),
        document: ComposerDocument {
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
        },
        then_close: false,
    })
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
async fn the_row_discard_removes_the_named_copy() {
    let provider = attached_provider();
    let removals = provider.draft_deletes();
    let app = app_over(provider).await;

    app.dispatch(Intent::Drafts(DraftsIntent::DiscardStored {
        message: draft_ref("d-1"),
    }))
    .await
    .unwrap();

    let taken = removals.lock().unwrap();
    assert_eq!(
        taken.as_slice(),
        [ProviderKey::new("d-1").unwrap()].as_slice(),
        "the row's discard removes exactly the named copy"
    );
}

#[tokio::test]
async fn the_row_discard_queues_the_removal_when_the_server_refuses() {
    let (draft, source) = attached_draft("d-1", "The named half", "Named words", 2);
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox(), fakes::folder("drafts", "Drafts")],
        vec![draft],
        vec![("d-1".to_owned(), source)],
    )
    .refusing_draft_deletes(1);
    let deletes = provider.draft_deletes();
    let app = app_over(provider).await;

    app.dispatch(Intent::Drafts(DraftsIntent::DiscardStored {
        message: draft_ref("d-1"),
    }))
    .await
    .unwrap();

    assert!(
        deletes.lock().unwrap().is_empty(),
        "the removal did not reach the server"
    );
    let outbox = app.engine.outbox(&account("work")).await.unwrap();
    assert!(
        outbox
            .iter()
            .any(|row| row.kind == Some(PendingOpKind::MailDraftDelete)),
        "the refused removal is queued, not dropped"
    );
}

#[tokio::test]
async fn the_row_discard_withdraws_a_save_still_waiting_for_a_network() {
    let provider = attached_provider().offlining_saves(1);
    let puts = provider.draft_puts();
    let deletes = provider.draft_deletes();
    let app = app_over(provider).await;

    // The resumed composition's save parks; the row's discard of the copy it
    // came from must withdraw it, or the parked words would store a draft the
    // user just deleted.
    app.resume_draft_by_key(&draft_ref("d-1"), composition("compose-1"))
        .await
        .expect("the named draft resumes");
    app.dispatch(save("compose-1", "forget me")).await.unwrap();
    app.dispatch(Intent::Drafts(DraftsIntent::DiscardStored {
        message: draft_ref("d-1"),
    }))
    .await
    .unwrap();

    let outbox = app.engine.outbox(&account("work")).await.unwrap();
    assert!(
        !outbox
            .iter()
            .any(|row| row.kind == Some(PendingOpKind::MailDraftPut)),
        "the queued save was withdrawn with the stored copy"
    );
    assert!(puts.lock().unwrap().is_empty());
    assert_eq!(
        deletes.lock().unwrap().as_slice(),
        [ProviderKey::new("d-1").unwrap()].as_slice(),
        "the named copy itself is still removed"
    );
}

#[tokio::test]
async fn the_row_discard_waits_on_the_save_gate() {
    let provider = attached_provider();
    let deletes = provider.draft_deletes();
    let app = app_over(provider).await;

    // A save stands mid-flight, holding the gate the composition discard
    // serialises on: the row's discard must queue behind it, or its
    // withdrawal and delete could slip between the save's steps and leave
    // the put storing a copy over the removal.
    let gate = Arc::clone(&app.save_gate);
    let saving = gate.lock().await;
    let discard = app.dispatch(Intent::Drafts(DraftsIntent::DiscardStored {
        message: draft_ref("d-1"),
    }));
    let mut pending = std::pin::pin!(discard);
    let raced = tokio::time::timeout(std::time::Duration::from_millis(50), pending.as_mut()).await;
    assert!(
        raced.is_err(),
        "the row's discard waits on the save gate, never racing a save"
    );
    drop(saving);
    pending.await.unwrap();

    assert_eq!(
        deletes.lock().unwrap().as_slice(),
        [ProviderKey::new("d-1").unwrap()].as_slice(),
        "the removal lands once the gate opens"
    );
}
