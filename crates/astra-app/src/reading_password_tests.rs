// SPDX-License-Identifier: GPL-3.0-only
//! The reading pane's password re-entry proofs (SP4c-3c D2): a SKESK message
//! opens needs-password, the correct passphrase re-enters to the words and the
//! restored subject (the prompt affordance drops), a wrong passphrase
//! re-prompts with no plaintext, and an app with no crypto store installed
//! stays needs-password (fail closed). The fixture hand-builds the SKESK
//! message via the backend's symmetric encrypt (W9 send is itself unwired),
//! the receive orchestrator's test pattern; the app harness is the decode
//! seam's own (`FakeProvider::with_sources`, one open, the snapshot read back).

use std::sync::{Arc, Mutex};

use astra_crypto_core::{
    mime, pgp_mime, CryptoBackend, CryptoPolicy, KeyGenParams, Part, PartId, PartKind, SecretBox,
    SerializationStrategy, Standard,
};
use astra_crypto_openpgp::OpenpgpBackend;
use astra_crypto_store::{CryptoStore, SqliteKeyStore};
use astra_viewmodel::PasswordPromptState;

use crate::{
    crypto_decode::CryptoDecoder, dispatch_fakes as fakes, App, Intent, MessageDecoder, MessageRef,
    Surface,
};

const RECIPIENT: &str = "work";
const PASS: &str = "hunter2";

fn test_store() -> Arc<CryptoStore> {
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account(RECIPIENT, "me@work.local").unwrap();
    Arc::new(store)
}

/// Seeds the recipient's own OpenPGP key: a private key is present, but the
/// SKESK-only fixture carries no PKESK slot it fits, so the open prompts.
async fn seed_own(store: &Arc<CryptoStore>) {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), RECIPIENT)),
        CryptoPolicy::default(),
    );
    backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "Work <me@work.local>".to_owned(),
            algorithm: "default".to_owned(),
            passphrase: None,
        })
        .await
        .expect("generate the recipient key");
}

fn passphrase(s: &str) -> SecretBox<String> {
    SecretBox::new(Box::new(s.to_owned()))
}

/// The full RFC5322 source for a SKESK PGP/MIME message under `pass`, the
/// protected real subject over both legs.
async fn symmetric_source(store: &Arc<CryptoStore>, pass: &str) -> Vec<u8> {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), RECIPIENT)),
        CryptoPolicy::default(),
    );
    let alternative = b"Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n\
        --alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nthe secret words\r\n\
        --alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
        <p>the secret words</p>\r\n--alt--\r\n";
    let inner = mime::wrap_protected_headers(alternative, Some("Subject: the real subject"), true);
    let env = backend
        .encrypt_symmetric(
            &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: inner,
            }],
            SerializationStrategy::SingleMimeBlob,
            passphrase(pass),
            None,
        )
        .await
        .expect("symmetric encrypt the fixture");
    let entity = pgp_mime::wrap_encrypted(&env.parts[0].ciphertext);
    let mut source = b"From: peer@x.com\r\nTo: me@work.local\r\nSubject: ...\r\n".to_vec();
    source.extend_from_slice(&entity);
    source
}

/// An app over the recipient holding the SKESK message, the built-in decoder
/// installed, and (when `install` holds) the crypto store the re-entry needs.
async fn app_with_source(install: bool) -> App<fakes::FakeProvider> {
    let store = test_store();
    seed_own(&store).await;
    let source = symmetric_source(&store, PASS).await;
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::mail("m1", "inbox", "...")],
        vec![("m1".to_owned(), source)],
    );
    let decoder: Arc<dyn MessageDecoder> = Arc::new(CryptoDecoder::new(store.clone()));
    let app = fakes::app(vec![fakes::account(RECIPIENT, provider)], &surfaces)
        .with_decoder(Some(decoder));
    if install {
        app.install_crypto_store(store);
    }
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

fn message() -> MessageRef {
    MessageRef::from_parts(RECIPIENT, "m1".to_owned()).unwrap()
}

async fn open(app: &App<fakes::FakeProvider>) {
    app.dispatch(Intent::OpenMessage { message: message() })
        .await
        .unwrap();
}

#[tokio::test]
async fn the_correct_passphrase_re_enters_to_the_words_and_the_restored_subject() {
    let app = app_with_source(true).await;
    open(&app).await;
    let closed = app.reading().expect("the open publishes a snapshot");
    assert_eq!(
        closed.password_prompt,
        Some(PasswordPromptState::NeedsPassword)
    );
    assert_eq!(closed.plain, None, "the open is fail closed");

    let prompt = app
        .submit_reading_password(message(), passphrase(PASS))
        .await;

    assert_eq!(prompt, None, "the decrypt drops the prompt affordance");
    let reading = app
        .reading()
        .expect("the re-entry republishes the snapshot");
    assert_eq!(reading.password_prompt, None, "the affordance is gone");
    assert_eq!(reading.plain.as_deref(), Some("the secret words"));
    assert!(
        reading
            .html
            .as_deref()
            .is_some_and(|body| body.contains("the secret words")),
        "the words render in the html leg: {:?}",
        reading.html
    );
    assert_eq!(
        reading.subject, "the real subject",
        "the protected subject restores"
    );
    assert!(
        !reading
            .attachments
            .iter()
            .any(|row| row.file_name.eq_ignore_ascii_case("encrypted.asc")),
        "the ciphertext chip hides: {:?}",
        reading.attachments
    );
}

#[tokio::test]
async fn a_wrong_passphrase_re_prompts_with_no_plaintext() {
    let app = app_with_source(true).await;
    open(&app).await;

    let prompt = app
        .submit_reading_password(message(), passphrase("not-the-passphrase"))
        .await;

    assert_eq!(prompt, Some(PasswordPromptState::WrongPassword));
    let reading = app
        .reading()
        .expect("the re-entry republishes the snapshot");
    assert_eq!(
        reading.password_prompt,
        Some(PasswordPromptState::WrongPassword),
        "the re-prompt signal rides the snapshot"
    );
    assert_eq!(
        reading.plain, None,
        "a wrong passphrase never yields plaintext"
    );
    assert_eq!(
        reading.html, None,
        "a wrong passphrase never yields ciphertext"
    );
    assert_eq!(reading.subject, "...", "no subject restores on a re-prompt");

    // The correct passphrase still opens after the refusal.
    let prompt = app
        .submit_reading_password(message(), passphrase(PASS))
        .await;
    assert_eq!(prompt, None);
    let reading = app.reading().expect("the snapshot stands");
    assert_eq!(reading.plain.as_deref(), Some("the secret words"));
}

#[tokio::test]
async fn no_crypto_store_installed_stays_needs_password() {
    let app = app_with_source(false).await;
    open(&app).await;

    let prompt = app
        .submit_reading_password(message(), passphrase(PASS))
        .await;

    assert_eq!(
        prompt,
        Some(PasswordPromptState::NeedsPassword),
        "nothing to decrypt against: the verdict stays needs-password"
    );
    let reading = app.reading().expect("the snapshot stands");
    assert_eq!(reading.plain, None, "no store never yields plaintext");
}
