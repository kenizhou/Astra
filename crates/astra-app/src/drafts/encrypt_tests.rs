// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the drafts-at-rest vault seam: the save that seals the mirror
//! before the engine's plaintext put and refuses when the mirror cannot be
//! written, the discard that takes the mirror with the copy, the resume that
//! prefers the vault words, falls back for a never-mirrored draft, and fails
//! closed for a mirror it cannot decrypt, and the kept-vs-not-kept decision
//! that decides whether a put error rolls the mirror back.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_crypto_store::{
    encrypted_draft_blob, get_encrypted_draft, put_encrypted_draft, CryptoStore,
};
use engine_api::{AccountId, ApiError, Draft, SyncError};
use engine_provider::ProviderError;

use super::{engine_kept_draft, mirror_delete, mirror_save, read_mirror, MirrorRead};
use crate::{dispatch_fakes as fakes, App, CompositionId, DraftsIntent, Intent};

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).expect("a composition id")
}

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

/// An app over one `work` account with the crypto store installed and the fixed
/// test master key armed, so the mirror wraps without touching the OS keyring.
fn app_with_store(
    provider: fakes::FakeProvider,
) -> (Arc<App<fakes::FakeProvider>>, Arc<CryptoStore>) {
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = Arc::new(App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        vec![fakes::account("work", provider)],
        None,
        Arc::new(fakes::RecordingObserver {
            surfaces: Arc::clone(&surfaces),
        }),
    ));
    app.install_crypto_store(Arc::clone(&store));
    (app, store)
}

// The kept-vs-not-kept decision: the outbox records the op before the provider
// call, so a provider failure parks the draft (kept) and an applied-but-
// unrecorded write is on the server (kept). Only the payload-encode failure
// happens before the op is enqueued (not kept). Every other arm is ambiguous
// and kept conservatively; the not-claimable Outbox arm stands in for the
// Store/Unrecorded arms that share the same catch-all.

#[test]
fn a_parked_provider_failure_keeps_the_draft() {
    let err = ApiError::Sync(SyncError::Provider(ProviderError::retryable(
        "no route to host",
    )));
    assert!(engine_kept_draft(&err), "a parked save kept the draft");
}

#[test]
fn a_payload_encode_failure_keeps_nothing() {
    let err = ApiError::Sync(SyncError::Outbox("encode draft put: boom".to_owned()));
    assert!(
        !engine_kept_draft(&err),
        "the op was never enqueued, so the engine kept nothing"
    );
}

#[test]
fn a_not_claimable_op_keeps_the_draft() {
    let err = ApiError::Sync(SyncError::Outbox(
        "enqueued op OpId(1) was not claimable: Busy".to_owned(),
    ));
    assert!(
        engine_kept_draft(&err),
        "the op was enqueued before the claim refused, so the draft is kept"
    );
}

#[tokio::test]
async fn a_save_seals_the_mirror_before_the_engine_put() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let (app, store) = app_with_store(provider);

    app.dispatch(save("compose-1", "sealed words"))
        .await
        .unwrap();

    let message_id = puts.lock().unwrap()[0].0.message_id.clone();
    let mirrored = get_encrypted_draft(&store, "work", message_id.as_str())
        .unwrap()
        .expect("the save wrote the mirror");
    let draft: Draft = serde_json::from_slice(&mirrored).unwrap();
    assert_eq!(
        draft.text_body, "sealed words",
        "the mirror holds the draft"
    );

    // The raw row is the vault blob, never the plaintext words.
    let raw = encrypted_draft_blob(&store, "work", message_id.as_str())
        .unwrap()
        .expect("the row is present");
    assert!(raw.starts_with("01"), "the v1 vault blob version byte");
    assert!(
        !raw.contains("sealed words") && !raw.contains(&hex::encode("sealed words")),
        "the plaintext never rides the stored row"
    );
}

#[tokio::test]
async fn a_save_refuses_when_the_mirror_cannot_be_written() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let (app, store) = app_with_store(provider);
    // A store that cannot take the mirror row: the write fails, so the save
    // refuses fail closed and never hands the engine a plaintext draft. (The
    // identical refusal path a vault error rides; a genuine keyring refusal is
    // not reproducible offline, the typed mapping is pinned in the store suite.)
    store
        .conn_for_test()
        .execute("DROP TABLE encrypted_drafts", [])
        .unwrap();

    app.dispatch(save("compose-1", "never persisted"))
        .await
        .unwrap();

    assert!(
        puts.lock().unwrap().is_empty(),
        "a refused save never reaches the engine's plaintext put"
    );
}

#[tokio::test]
async fn a_parked_save_keeps_the_mirror() {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]).offlining_saves(1);
    let (app, store) = app_with_store(provider);

    app.dispatch(save("compose-1", "offline words"))
        .await
        .unwrap();

    // The provider refused retryably, so the engine parked the op (kept the
    // draft); the mirror survives the parked save under the composition's own
    // Message-ID.
    let message_id = app
        .composition_message_id(&composition("compose-1"))
        .expect("the parked save still names its composition");
    assert!(
        get_encrypted_draft(&store, "work", message_id.as_str())
            .unwrap()
            .is_some(),
        "the mirror survives a parked save"
    );
}

#[tokio::test]
async fn a_discard_removes_the_mirror() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let (app, store) = app_with_store(provider);

    app.dispatch(save("compose-1", "forget me")).await.unwrap();
    let message_id = puts.lock().unwrap()[0].0.message_id.clone();
    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("compose-1"),
    }))
    .await
    .unwrap();

    assert_eq!(
        get_encrypted_draft(&store, "work", message_id.as_str()).unwrap(),
        None,
        "the discard took the mirror with the stored copy"
    );
}

#[tokio::test]
async fn a_resume_prefers_the_vault_words_over_the_engine_body() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let (app, store) = app_with_store(provider);

    app.dispatch(save("compose-1", "engine words"))
        .await
        .unwrap();
    let message_id = puts.lock().unwrap()[0].0.message_id.clone();
    // Overwrite the mirror with words the engine body does not carry: a resume
    // that answers them reads the vault, not the engine cache.
    let mut draft = puts.lock().unwrap()[0].0.clone();
    draft.text_body = "words only the vault holds".to_owned();
    let json = serde_json::to_vec(&draft).unwrap();
    put_encrypted_draft(&store, "work", message_id.as_str(), &json).unwrap();

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let resumed = app
        .resume_latest_draft(&account("work"), composition("compose-2"))
        .await
        .expect("the stored draft resumes");
    assert!(
        resumed.body_text.contains("words only the vault holds"),
        "the resume reads the vault words: {}",
        resumed.body_text
    );
    assert!(
        !resumed.body_text.contains("engine words"),
        "the engine body is not the source when the mirror stands"
    );
}

#[tokio::test]
async fn a_never_mirrored_draft_resumes_from_the_engine_body() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let (app, _store) = app_with_store(provider);

    // A save with no mirror row (the draft was written elsewhere): delete the
    // mirror the save wrote, leaving the engine body as the only copy.
    app.dispatch(save("compose-1", "plain fallback words"))
        .await
        .unwrap();
    let message_id = puts.lock().unwrap()[0].0.message_id.clone();
    mirror_delete(&app, &account("work"), message_id.as_str());

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let resumed = app
        .resume_latest_draft(&account("work"), composition("compose-2"))
        .await
        .expect("the never-mirrored draft resumes");
    assert!(
        resumed.body_text.contains("plain fallback words"),
        "an absent mirror falls back to the engine body: {}",
        resumed.body_text
    );
}

#[tokio::test]
async fn an_undecryptable_mirror_fails_closed() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let puts = provider.draft_puts();
    let (app, store) = app_with_store(provider);

    app.dispatch(save("compose-1", "secret words"))
        .await
        .unwrap();
    let message_id = puts.lock().unwrap()[0].0.message_id.clone();
    // Overwrite the mirror with bytes that are not a draft: the mirror exists
    // but is unreadable, so the resume fails closed, never the plaintext
    // fallback. (The AEAD refusal arm is pinned in the store suite; both map to
    // the same fail-closed answer here.)
    put_encrypted_draft(&store, "work", message_id.as_str(), b"not a draft").unwrap();

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let resumed = app
        .resume_latest_draft(&account("work"), composition("compose-2"))
        .await;
    assert!(resumed.is_none(), "an undecryptable mirror never resumes");
    let banner = app
        .reading()
        .expect("the fail-closed banner rides the reading snapshot")
        .security_banner
        .expect("the banner is present");
    assert!(
        banner.text.contains("cannot decrypt"),
        "the banner names the refusal: {}",
        banner.text
    );
    assert!(
        app.reading().unwrap().plain.is_none(),
        "the fail-closed answer carries no plaintext"
    );
}

#[tokio::test]
async fn the_read_seam_answers_absent_without_a_store() {
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox(), fakes::folder("drafts", "Drafts")]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        vec![fakes::account("work", provider)],
        None,
        Arc::new(fakes::RecordingObserver {
            surfaces: Arc::clone(&surfaces),
        }),
    );
    // No store installed: the mirror read answers absent (the engine-body
    // fallback), and a save proceeds as a plain draft.
    let stored = fakes::mail("d-1", "drafts", "A draft");
    assert!(matches!(
        read_mirror(&app, &account("work"), &stored),
        MirrorRead::Absent
    ));
    let draft = Draft::new(
        engine_api::MessageIdHeader::new("d-1@x").unwrap(),
        engine_api::EmailAddress::new("me@work.local"),
        Vec::new(),
        "subject",
        "words",
    );
    assert!(mirror_save(&app, &account("work"), &draft).is_ok());
}
