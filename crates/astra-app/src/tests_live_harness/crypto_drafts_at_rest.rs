// SPDX-License-Identifier: GPL-3.0-only
//! The gated drafts-at-rest proof (SP4c-4): with the test master key armed, a
//! draft saved through the product's own dispatch writes an encrypted mirror
//! whose row is ciphertext (never the plaintext words), the resume reads the
//! words back from the vault, and the discard removes the mirror. The draft
//! key is the composition's stable Message-ID, the same identity the resume
//! derives from the stored message; this proof pins that agreement over the
//! real server. Every copy is withdrawn, so the harness keeps its seeded shape.

use std::sync::Arc;

use astra_composer::ComposerDocument;
use astra_crypto_store::{encrypted_draft_blob, get_encrypted_draft, CryptoStore};
use astra_viewmodel::{FolderRole, RowKey};
use engine_api::{EmailAddress, Engine};

use crate::{
    reference::MessageRef, Account, App, AppObserver, CompositionId, DraftsIntent, FolderRef,
    Intent,
};

use super::{dial_with_accepted_certificate, harness, proof, row_by_subject, CountingObserver};

/// The subject the stored copy carries: unique among the harness fixtures.
const SUBJECT: &str = "Harness drafts-at-rest proof";

/// The words the draft carries; the mirror seals them and the resume reads them.
const WORDS: &str = "Sealed at rest for the drafts proof, opened from the vault.";

fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).expect("a composition id")
}

/// The Drafts folder of `account`, as the pane projects it.
fn drafts_folder(
    app: &App<Box<dyn engine_api::Provider>>,
    account: &engine_api::AccountId,
) -> FolderRef {
    FolderRef::from_parts(
        account.as_str(),
        proof::role_key(app, account, FolderRole::Drafts),
    )
    .expect("the Drafts folder reference")
}

/// The drafts the account's store holds.
async fn stored_drafts(
    app: &App<Box<dyn engine_api::Provider>>,
    account: &engine_api::AccountId,
) -> Vec<engine_api::Message> {
    app.engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(engine_api::Message::is_draft)
        .collect()
}

/// The gated run: a saved draft's mirror row is ciphertext, the resume answers
/// the vault words, and the discard removes the mirror. Every copy is withdrawn.
#[tokio::test]
async fn the_draft_mirror_is_ciphertext_and_resumes_from_the_vault() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    // The fixed test master key keeps the proof off the OS keyring.
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    let (account, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    store
        .register_account(account.as_str(), &user)
        .expect("the account row stands");
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account.clone(),
            providers,
            identity: EmailAddress::new(&user),
        }],
        None,
        observer,
    );
    app.install_crypto_store(Arc::clone(&store));

    // Back to a known state: sweep any earlier run's drafts first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let mut swept = 0;
    while app
        .resume_latest_draft(&account, composition("sweep"))
        .await
        .is_some()
    {
        app.dispatch(Intent::Drafts(DraftsIntent::Discard {
            composition: composition("sweep"),
        }))
        .await
        .unwrap();
        app.dispatch(Intent::RefreshMail).await.unwrap();
        swept += 1;
        assert!(
            swept < 8,
            "the sweep discards more drafts than any earlier run leaves"
        );
    }

    // The save through the product's own dispatch: the mirror is sealed before
    // the engine's plaintext put.
    app.dispatch(Intent::Drafts(DraftsIntent::Save {
        composition: composition("rest"),
        from: account.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: ComposerDocument {
            blocks: vec![proof::paragraph(WORDS)],
            attachments: Vec::new(),
        },
        then_close: false,
    }))
    .await
    .unwrap();

    // The stored copy presents after a pass; its Message-ID is the mirror key,
    // the same identity the save wrote and the resume adopts.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::SelectFolder {
        folder: drafts_folder(&app, &account),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let RowKey::Message { key, .. } = row_by_subject(&snapshot, SUBJECT).key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    let message = MessageRef {
        account: account.clone(),
        key,
    };
    let stored = crate::reading::resolve(&app.engine, &message)
        .await
        .expect("the stored draft resolves");
    let draft_key = stored
        .envelope
        .message_id
        .first()
        .expect("the stored draft carries its Message-ID")
        .as_str()
        .to_owned();

    // The mirror row is ciphertext, never the plaintext words.
    let raw = encrypted_draft_blob(&store, account.as_str(), &draft_key)
        .expect("the mirror read")
        .expect("the save wrote the mirror");
    assert!(raw.starts_with("01"), "the v1 vault blob version byte");
    assert!(
        !raw.contains(WORDS) && !raw.contains(&hex::encode(WORDS)),
        "the plaintext never rides the stored row"
    );
    let mirrored = get_encrypted_draft(&store, account.as_str(), &draft_key)
        .expect("the mirror decrypts")
        .expect("the mirror is present");
    let draft: engine_api::Draft =
        serde_json::from_slice(&mirrored).expect("the mirror is a draft");
    assert_eq!(draft.text_body, WORDS, "the vault holds the words");

    // The resume reads the words from the vault.
    let resumed = app
        .resume_draft_by_key(&message, composition("reopen"))
        .await
        .expect("the named copy resumes");
    assert!(
        resumed.body_text.contains(WORDS),
        "the resumed body is the vault words: {}",
        resumed.body_text
    );

    // The discard removes the mirror with the stored copy.
    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("reopen"),
    }))
    .await
    .unwrap();
    assert_eq!(
        encrypted_draft_blob(&store, account.as_str(), &draft_key).expect("the mirror read"),
        None,
        "the discard removed the mirror"
    );

    // The cleanup: the Drafts folder back to the empty shape the suites expect.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let left = stored_drafts(&app, &account).await;
    assert!(
        left.is_empty(),
        "the discard leaves the account holding no draft"
    );
}
