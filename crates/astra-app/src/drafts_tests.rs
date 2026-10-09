// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the drafts family: the save that stores a copy the next save supersedes,
//! the discard that removes it, the close that saves what the composer held, the resume
//! that opens the account's latest stored draft, and the idle autosave that retries an
//! unsettled save once the composer has gone quiet. The fake answers draft saves with a
//! key that moves on every save, which is what three of the four real adapters do, so a
//! caller that never names the copy it replaces cannot pass these tests by accident.
//!
//! The autosave tests run on paused tokio time, so thirty seconds of idleness costs
//! nothing and the "not before the idle" half of the contract is a clock reading rather
//! than a sleep the suite would flake on.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::DraftSaveState;
use engine_api::{
    AccountId, EmailAddress, Message, MessageIdHeader, PendingOpKind, ProviderKey, SystemKeyword,
    UtcDateTime,
};

use crate::{
    dispatch_fakes as fakes, drafts::DRAFT_AUTOSAVE_IDLE, App, CompositionId, DraftsIntent, Intent,
    Surface,
};

/// A well-formed account id, the way the drafts tests name one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A composition id, the way every drafts test names one.
fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).expect("a composition id")
}

/// A one-paragraph document carrying `text`.
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

/// What a composer dispatches when its Save button is pressed or its idle timer fires;
/// the core cannot tell the two apart, and nothing here should.
fn save(id: &str, text: &str) -> Intent {
    Intent::Drafts(DraftsIntent::Save {
        composition: composition(id),
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Later".to_owned(),
        document: document(text),
        then_close: false,
    })
}

/// A draft an earlier session stored on the server, as a `(message, raw source)` pair the
/// fake serves from construction: the shape a Drafts folder holds between sessions.
fn stored_draft(key: &str, subject: &str, body: &str, minute: u8) -> (Message, Vec<u8>) {
    let mut message = fakes::mail(key, "drafts", subject);
    message
        .keywords
        .insert(engine_api::Keyword::system(SystemKeyword::Draft));
    message.envelope.to = vec![EmailAddress::new("you@test.local")];
    message.envelope.message_id =
        vec![MessageIdHeader::new(format!("{key}@server.local")).unwrap()];
    message.received_at = Some(UtcDateTime::new(2026, 10, 9, 9, minute, 0).unwrap());
    let source =
        format!("From: me@work.local\r\nTo: you@test.local\r\nSubject: {subject}\r\n\r\n{body}");
    (message, source.into_bytes())
}

/// An in-memory app over one `work` account, recording signalled surfaces into
/// `surfaces`, with the autosave idle at `idle` so the paused-clock tests need no real
/// time and the product default costs nothing.
fn app_over(
    provider: fakes::FakeProvider,
    idle: Duration,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> Arc<App<fakes::FakeProvider>> {
    Arc::new(
        App::new(
            Arc::new(engine_api::Engine::open_in_memory().unwrap()),
            vec![fakes::account("work", provider)],
            None,
            Arc::new(fakes::RecordingObserver {
                surfaces: Arc::clone(surfaces),
            }),
        )
        .with_draft_autosave_idle(idle),
    )
}

// The autosave tests run on paused time in their own file, under this module.
#[path = "drafts_autosave_tests.rs"]
mod autosave;

/// The provider keys of every draft the provider was asked to remove.
fn deleted(deletes: &Arc<Mutex<Vec<ProviderKey>>>) -> Vec<String> {
    deletes
        .lock()
        .unwrap()
        .iter()
        .map(|key| key.as_str().to_owned())
        .collect()
}

#[tokio::test]
async fn a_save_persists_and_reads_back() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "Half written"))
        .await
        .unwrap();

    {
        let puts = puts.lock().unwrap();
        assert_eq!(puts.len(), 1, "the save stores the draft");
        assert_eq!(puts[0].1, None, "a first save supersedes nothing");
        assert_eq!(puts[0].0.subject, "Later");
    }
    let status = app.draft_status().expect("a save reports its outcome");
    assert_eq!(status.composition, "compose-1");
    assert_eq!(status.state, DraftSaveState::Saved);
    assert!(
        !status.saved_at.is_empty(),
        "the landing instant is stamped"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::DraftStatus),
        "the transition is signalled"
    );

    // The server's Drafts folder holds the copy; a refresh presents it to the store, and
    // the resume read opens it back.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let resumed = app
        .resume_latest_draft(&account("work"), composition("compose-2"))
        .await
        .expect("the stored draft resumes");
    assert_eq!(resumed.to, "you@test.local");
    assert_eq!(resumed.subject, "Later");
    assert!(
        resumed.body_text.contains("Half written"),
        "the resumed body is the words that were saved"
    );
}

#[tokio::test]
async fn a_re_save_names_the_copy_it_supersedes() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "first")).await.unwrap();
    app.dispatch(save("compose-1", "second")).await.unwrap();

    let puts = puts.lock().unwrap();
    assert_eq!(puts.len(), 2, "each save stores the draft");
    assert_eq!(
        puts[1].1,
        Some(ProviderKey::new("draft-1").unwrap()),
        "the second save must name what the first stored, or the server keeps both"
    );
    assert_eq!(
        puts[0].0.message_id, puts[1].0.message_id,
        "two saves of one composition are the same message"
    );
}

#[tokio::test]
async fn an_unchanged_save_reaches_no_server() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "same")).await.unwrap();
    app.dispatch(save("compose-1", "same")).await.unwrap();

    assert_eq!(
        puts.lock().unwrap().len(),
        1,
        "an idle fire on a composer nobody touched must not write"
    );
    assert_eq!(
        app.draft_status().map(|status| status.state),
        Some(DraftSaveState::Saved),
        "the words are on the server, which is what the composer should say"
    );
}

#[tokio::test]
async fn two_composers_supersede_only_their_own_draft() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "one")).await.unwrap();
    app.dispatch(save("compose-2", "two")).await.unwrap();
    app.dispatch(save("compose-1", "one again")).await.unwrap();

    let puts = puts.lock().unwrap();
    assert_eq!(puts.len(), 3);
    assert_eq!(
        puts[1].1, None,
        "the second composer's first save is its own"
    );
    assert_eq!(
        puts[2].1,
        Some(ProviderKey::new("draft-1").unwrap()),
        "the first composer's re-save must name its own stored copy, not the other's"
    );
    assert_ne!(
        puts[0].0.message_id, puts[1].0.message_id,
        "a different composition is a different message"
    );
}

#[tokio::test]
async fn a_discard_removes_the_stored_copy() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let puts = provider.draft_puts();
    let deletes = provider.draft_deletes();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "never mind")).await.unwrap();
    app.dispatch(save("compose-1", "really never mind"))
        .await
        .unwrap();
    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("compose-1"),
    }))
    .await
    .unwrap();

    assert_eq!(
        deleted(&deletes),
        vec!["draft-2".to_owned()],
        "the removal must name the copy that is actually there, which the last save moved"
    );
    assert!(
        app.draft_status().is_none(),
        "a discarded composition leaves no hint behind"
    );
    assert_eq!(
        puts.lock().unwrap().len(),
        2,
        "discarding stores nothing new"
    );
}

#[tokio::test]
async fn a_discard_withdraws_a_save_still_waiting_for_a_network() {
    let provider = fakes::FakeProvider::saving_nothing_for(1);
    let puts = provider.draft_puts();
    let deletes = provider.draft_deletes();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "forget it")).await.unwrap();
    assert!(puts.lock().unwrap().is_empty());

    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("compose-1"),
    }))
    .await
    .unwrap();

    assert!(
        puts.lock().unwrap().is_empty(),
        "the queued save must not store a draft the user discarded"
    );
    assert!(deleted(&deletes).is_empty(), "nothing was ever stored");
    let outbox = app.engine.outbox(&account("work")).await.unwrap();
    assert!(
        !outbox
            .iter()
            .any(|row| row.kind == Some(PendingOpKind::MailDraftPut)),
        "the queued save was withdrawn with the composition"
    );
}

#[tokio::test]
async fn a_close_saves_what_the_composer_held_and_clears_the_hint() {
    let provider = fakes::FakeProvider::saving_nothing_for(1);
    let puts = provider.draft_puts();
    let deletes = provider.draft_deletes();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    // The save had no network; the close hands the same words to the outbox again and
    // this time they go out, then the composition is forgotten.
    app.dispatch(save("compose-1", "keep this")).await.unwrap();
    app.dispatch(Intent::Drafts(DraftsIntent::Close {
        composition: composition("compose-1"),
    }))
    .await
    .unwrap();

    {
        let puts_now = puts.lock().unwrap();
        assert_eq!(puts_now.len(), 1, "the close saved what the composer held");
        assert!(puts_now[0].0.text_body.contains("keep this"));
    }
    assert!(
        deleted(&deletes).is_empty(),
        "a close never removes a draft"
    );
    assert!(
        app.draft_status().is_none(),
        "the composition is forgotten, hint and all"
    );

    // A host reusing the id starts a new draft, not a save over the closed one.
    app.dispatch(save("compose-1", "a different message"))
        .await
        .unwrap();
    let puts_now = puts.lock().unwrap();
    assert_eq!(puts_now.len(), 2);
    assert_eq!(
        puts_now[1].1, None,
        "a reused id must supersede nothing, not the copy the close left in Drafts"
    );
}

#[tokio::test]
async fn a_save_that_closes_stores_the_draft_and_forgets_the_composition() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let puts = provider.draft_puts();
    let deletes = provider.draft_deletes();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(Intent::Drafts(DraftsIntent::Save {
        composition: composition("compose-1"),
        from: account("work"),
        to: String::new(),
        cc: String::new(),
        bcc: String::new(),
        subject: String::new(),
        document: document("typed on the way out"),
        then_close: true,
    }))
    .await
    .unwrap();

    assert_eq!(puts.lock().unwrap().len(), 1, "what was typed is kept");
    assert!(
        deleted(&deletes).is_empty(),
        "leaving never removes the draft"
    );
    assert!(
        app.draft_status().is_none(),
        "the composition is forgotten, hint and all"
    );
}

#[tokio::test]
async fn a_discard_of_something_never_saved_reaches_no_server() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let deletes = provider.draft_deletes();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("compose-1"),
    }))
    .await
    .unwrap();

    assert!(
        deleted(&deletes).is_empty(),
        "there is nothing on the server to remove"
    );
}

#[tokio::test]
async fn a_resume_restores_the_latest_draft_and_saves_over_it() {
    let (older, older_source) = stored_draft("old-1", "An older half", "Older words", 1);
    let (newer, newer_source) = stored_draft("new-1", "The newest half", "Newest words", 2);
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox(), fakes::folder("drafts", "Drafts")],
        vec![older, newer],
        vec![
            ("old-1".to_owned(), older_source),
            ("new-1".to_owned(), newer_source),
        ],
    );
    let puts = provider.draft_puts();
    let deletes = provider.draft_deletes();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let resumed = app
        .resume_latest_draft(&account("work"), composition("compose-1"))
        .await
        .expect("the newest stored draft resumes");
    assert_eq!(resumed.subject, "The newest half");
    assert_eq!(resumed.to, "you@test.local");
    assert!(
        resumed.body_text.contains("Newest words"),
        "the resumed body is the newest copy's words"
    );

    // The resumed composition saves over the copy it came from, and a discard removes
    // that same copy, not a second one beside it.
    app.dispatch(save("compose-1", "resumed and edited"))
        .await
        .unwrap();
    assert_eq!(
        puts.lock().unwrap()[0].1,
        Some(ProviderKey::new("new-1").unwrap()),
        "the first save after a resume supersedes the copy it opened"
    );
    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("compose-1"),
    }))
    .await
    .unwrap();
    assert_eq!(
        deleted(&deletes),
        vec!["draft-1".to_owned()],
        "the removal names the copy the re-save moved to, not the one it superseded"
    );
}

#[tokio::test]
async fn the_status_surface_publishes_each_transition() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, DRAFT_AUTOSAVE_IDLE, &surfaces);

    app.dispatch(save("compose-1", "mine")).await.unwrap();

    let published = surfaces
        .lock()
        .unwrap()
        .iter()
        .filter(|surface| **surface == Surface::DraftStatus)
        .count();
    assert!(
        published >= 2,
        "a save that lands moves through saving to saved, and says so each time"
    );
    assert_eq!(puts.lock().unwrap().len(), 1);
}
