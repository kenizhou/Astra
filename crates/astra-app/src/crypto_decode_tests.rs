// SPDX-License-Identifier: GPL-3.0-only
//! The built-in crypto decoder's proofs through the reading pipeline: a
//! PGP/MIME source decrypts to the words with a verified banner and the
//! restored protected subject, the no-key / unknown-signer / tampered arms
//! carry their banners fail closed (no plaintext, no ciphertext chips), a
//! non-encrypted source is left untouched, and a crypto verdict owns the
//! whole body even when the store's own read carries words.
//!
//! The send half's own primitives build the fixtures (the receive
//! orchestrator's test pattern): a sign-then-encrypt envelope wrapped by
//! `pgp_mime::wrap_encrypted`, the exact shape the product send path emits.
//! The app harness is the decode seam's own: `FakeProvider::with_sources`
//! serving the raw RFC5322, one `Intent::OpenMessage`, the snapshot read back.

use std::sync::{Arc, Mutex};

use astra_crypto_core::{
    mime, pgp_mime, CryptoBackend, CryptoPolicy, EncryptOp, KeyGenParams, KeyHandleRef, Part,
    PartId, PartKind, SerializationStrategy, Standard,
};
use astra_crypto_openpgp::{engine, keymap, OpenpgpBackend};
use astra_crypto_store::{CryptoKeyRecord, CryptoKeyRow, CryptoStore, SqliteKeyStore};
use astra_viewmodel::{ReadingSnapshot, SecurityBannerView, SecurityLevelView};

use crate::{
    crypto_decode::CryptoDecoder, dispatch_fakes as fakes, App, DecodeOutcome, Intent,
    MessageDecoder, MessageRef, Surface,
};

/// The verified banner the decrypted arm carries.
const VERIFIED: &str = "Encrypted message; signature verified.";
/// The unknown-signer banner: decrypted, but the signature did not verify.
const UNKNOWN_SIGNER: &str = "Encrypted message; signature unverified (unknown signer).";
/// The no-key banner (fail closed).
const NO_KEY: &str = "Encrypted message; cannot decrypt (no key).";
/// The failed-decrypt banner (fail closed).
const FAILED: &str = "Encrypted message; cannot decrypt.";

/// The sender's account in the fixture store; the recipient is the app's
/// `work` account, the one the reading pipeline resolves.
const SENDER: &str = "peer";
const RECIPIENT: &str = "work";

fn test_store() -> Arc<CryptoStore> {
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account(SENDER, "peer@x.com").unwrap();
    store.register_account(RECIPIENT, "me@work.local").unwrap();
    Arc::new(store)
}

/// Seeds an account's own OpenPGP key (private material at rest, default
/// signer) and returns its canonical handle.
async fn seed_own(store: &Arc<CryptoStore>, account: &str, email: &str) -> KeyHandleRef {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account)),
        CryptoPolicy::default(),
    );
    let handle = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: format!("Peer <{email}>"),
            algorithm: "default".to_string(),
            passphrase: None,
        })
        .await
        .expect("generate own pgp key");
    astra_crypto_store::set_default_signing_key(
        store,
        account,
        "openpgp",
        handle.fingerprint.as_str(),
    )
    .unwrap();
    handle
}

/// The stored public blob of an account's key, for cross-account import.
fn public_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let record = astra_crypto_store::get_crypto_key_full(store, account, "openpgp", fingerprint)
        .unwrap()
        .expect("the key row exists");
    hex::decode(record.public_data).expect("public blob hex")
}

/// Imports a public cert into an account as a contact (no private material).
fn import_contact(store: &Arc<CryptoStore>, account: &str, email: &str, blob: &[u8]) {
    let certs = keymap::parse_certs(blob).expect("parse contact cert");
    let fp = keymap::fingerprint_of(&certs[0]).as_str().to_string();
    astra_crypto_store::upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: account.to_string(),
                standard: "openpgp".to_string(),
                key_type: "cert".to_string(),
                email: Some(email.to_string()),
                fingerprint: fp,
                origin: "contact".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(blob),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
}

/// Builds the full RFC5322 source the reading pipeline hands the decoder:
/// outer headers (Subject placeholder when protected) plus the PGP/MIME
/// `multipart/encrypted` entity framing a sign-then-encrypt envelope.
async fn encrypt_source(
    store: &Arc<CryptoStore>,
    recipient: &KeyHandleRef,
    signer: Option<&KeyHandleRef>,
    inner_entity: &[u8],
    outer_subject: &str,
) -> Vec<u8> {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), SENDER)),
        CryptoPolicy::default(),
    );
    let env = backend
        .encrypt(EncryptOp {
            parts: &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: inner_entity.to_vec(),
            }],
            serialization: SerializationStrategy::SingleMimeBlob,
            recipients: std::slice::from_ref(recipient),
            sign_with: signer.cloned(),
        })
        .await
        .expect("encrypt the fixture");
    let ciphertext = env.parts[0].ciphertext.clone();
    let entity = pgp_mime::wrap_encrypted(&ciphertext);
    let mut source =
        format!("From: peer@x.com\r\nTo: me@work.local\r\nSubject: {outer_subject}\r\n")
            .into_bytes();
    source.extend_from_slice(&entity);
    source
}

/// The decrypted inner entity: both legs, the HTML carrying a script the
/// sanitise pipeline must strip, under the protected real subject.
fn protected_inner() -> Vec<u8> {
    let alternative = b"Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n\
        --alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nthe secret words\r\n\
        --alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
        <p>the secret words</p><script>steal()</script>\r\n--alt--\r\n";
    mime::wrap_protected_headers(alternative, Some("Subject: the real subject"), true)
}

/// The two-account fixture: peer sends, work receives. `verify` imports
/// peer's public cert into work so the embedded signature verifies;
/// otherwise work holds no signer cert and the signature degrades to
/// unknown-key.
async fn two_accounts(verify: bool) -> (Arc<CryptoStore>, KeyHandleRef, KeyHandleRef) {
    let store = test_store();
    let peer = seed_own(&store, SENDER, "peer@x.com").await;
    let work = seed_own(&store, RECIPIENT, "me@work.local").await;
    import_contact(
        &store,
        SENDER,
        "me@work.local",
        &public_blob(&store, RECIPIENT, work.fingerprint.as_str()),
    );
    if verify {
        import_contact(
            &store,
            RECIPIENT,
            "peer@x.com",
            &public_blob(&store, SENDER, peer.fingerprint.as_str()),
        );
    }
    (store, peer, work)
}

/// Corrupts one base64 character deep in the ciphertext body so the SEIPD
/// integrity check fails on decrypt (the orchestrator tests' own tamper).
fn tamper_ciphertext(source: &mut [u8]) {
    let marker = b"Content-Transfer-Encoding: base64";
    let pos = source
        .windows(marker.len())
        .position(|w| w == marker)
        .expect("the ciphertext part is base64");
    let after = pos + marker.len();
    let blank = source[after..]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a header/body blank line");
    let start = after + blank + 4;
    let end = source[start..]
        .windows(4)
        .position(|w| w == b"\r\n--")
        .expect("the closing boundary");
    let idx = start + end / 2;
    source[idx] = if source[idx] == b'A' { b'B' } else { b'A' };
}

/// An in-memory app over the `work` account holding one message whose raw
/// source is `source`, with the built-in crypto decoder installed.
async fn app_with_source(
    store: Arc<CryptoStore>,
    source: Vec<u8>,
    subject: &str,
) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let message = fakes::mail("m1", "inbox", subject);
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![message],
        vec![("m1".to_owned(), source)],
    );
    let decoder: Arc<dyn MessageDecoder> = Arc::new(CryptoDecoder::new(store));
    let app = fakes::app(vec![fakes::account(RECIPIENT, provider)], &surfaces)
        .with_decoder(Some(decoder));
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// Opens the fixture message and answers the reading snapshot.
async fn open(app: &App<fakes::FakeProvider>) -> ReadingSnapshot {
    app.dispatch(Intent::OpenMessage {
        message: MessageRef::from_parts(RECIPIENT, "m1".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    app.reading()
        .expect("the open publishes a reading snapshot")
}

/// Whether the snapshot carries a ciphertext pseudo-attachment chip.
fn has_crypto_chip(reading: &ReadingSnapshot) -> bool {
    reading.attachments.iter().any(|row| {
        row.file_name.eq_ignore_ascii_case("encrypted.asc")
            || row
                .media_type
                .eq_ignore_ascii_case("application/pgp-encrypted")
    })
}

/// The persisted result row for the fixture message, through the store's
/// public reader.
fn read_result(
    store: &Arc<CryptoStore>,
    message: &str,
) -> Option<astra_crypto_store::MessageCryptoResult> {
    astra_crypto_store::get_message_crypto_result(store, RECIPIENT, message)
}

#[tokio::test]
async fn a_pgp_mime_message_decrypts_through_the_reading_pipeline() {
    let (store, peer, work) = two_accounts(true).await;
    let source = encrypt_source(&store, &work, Some(&peer), &protected_inner(), "...").await;
    let app = app_with_source(store.clone(), source, "...").await;

    let reading = open(&app).await;

    assert_eq!(reading.plain.as_deref(), Some("the secret words"));
    let html = reading.html.as_deref().expect("the decrypted html leg");
    assert!(
        html.contains("the secret words"),
        "the words render: {html}"
    );
    assert!(
        !html.contains("script"),
        "the decrypted html is sanitised: {html}"
    );
    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Info,
            text: VERIFIED.to_owned(),
        })
    );
    assert_eq!(
        reading.subject, "the real subject",
        "the protected subject restores"
    );
    assert!(
        !has_crypto_chip(&reading),
        "the ciphertext chip hides: {:?}",
        reading.attachments
    );
    let row = read_result(&store, "m1").expect("the result row lands keyed by account and key");
    assert_eq!(row.crypto_kind, "encrypted-signed");
    assert_eq!(row.decrypt_state, "ok");
    assert_eq!(row.signature_state, "valid-verified");
    assert_eq!(
        row.signer_fingerprint.as_deref(),
        Some(peer.fingerprint.as_str())
    );
}

#[tokio::test]
async fn the_ciphertext_chip_renders_without_the_crypto_decoder() {
    let (store, peer, work) = two_accounts(true).await;
    let source = encrypt_source(&store, &work, Some(&peer), &protected_inner(), "...").await;
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::mail("m1", "inbox", "...")],
        vec![("m1".to_owned(), source)],
    );
    let app = fakes::app(vec![fakes::account(RECIPIENT, provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let reading = open(&app).await;

    assert!(
        has_crypto_chip(&reading),
        "the fixture's framing lists the ciphertext chip: {:?}",
        reading.attachments
    );
}

#[tokio::test]
async fn no_key_renders_the_banner_and_no_plaintext() {
    let store = test_store();
    let peer = seed_own(&store, SENDER, "peer@x.com").await;
    // work holds a PUBLIC cert only: peer can encrypt to it, but work cannot
    // decrypt.
    let work_cert = engine::generate("Work <me@work.local>").unwrap();
    let work_public = keymap::cert_to_public_blob(&work_cert).unwrap();
    import_contact(&store, RECIPIENT, "me@work.local", &work_public);
    import_contact(&store, SENDER, "me@work.local", &work_public);
    let work_handle = keymap::cert_to_handle(&work_cert);
    let source = encrypt_source(&store, &work_handle, Some(&peer), &protected_inner(), "...").await;
    let app = app_with_source(store.clone(), source, "...").await;

    let reading = open(&app).await;

    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Error,
            text: NO_KEY.to_owned(),
        })
    );
    assert_eq!(reading.plain, None, "no key never yields plaintext");
    assert_eq!(reading.html, None, "no key never yields ciphertext");
    assert_eq!(
        reading.subject, "...",
        "no subject restores on a closed arm"
    );
    assert!(!has_crypto_chip(&reading), "the ciphertext chip hides");
    assert_eq!(
        read_result(&store, "m1").map(|row| row.decrypt_state),
        Some("no-key".to_owned())
    );
}

#[tokio::test]
async fn an_unknown_signer_decrypts_with_an_unverified_banner() {
    let (store, peer, work) = two_accounts(false).await;
    let source = encrypt_source(&store, &work, Some(&peer), &protected_inner(), "...").await;
    let app = app_with_source(store, source, "...").await;

    let reading = open(&app).await;

    assert_eq!(reading.plain.as_deref(), Some("the secret words"));
    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Warning,
            text: UNKNOWN_SIGNER.to_owned(),
        })
    );
    assert_eq!(reading.subject, "the real subject");
}

#[tokio::test]
async fn a_tampered_ciphertext_fails_closed() {
    let (store, peer, work) = two_accounts(true).await;
    let mut source = encrypt_source(&store, &work, Some(&peer), &protected_inner(), "...").await;
    tamper_ciphertext(&mut source);
    let app = app_with_source(store.clone(), source, "...").await;

    let reading = open(&app).await;

    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Error,
            text: FAILED.to_owned(),
        })
    );
    assert_eq!(reading.plain, None, "a failure never yields plaintext");
    assert_eq!(reading.html, None, "a failure never yields ciphertext");
    assert!(!has_crypto_chip(&reading), "the ciphertext chip hides");
    assert_eq!(
        read_result(&store, "m1").map(|row| row.decrypt_state),
        Some("failed".to_owned())
    );
}

#[tokio::test]
async fn a_non_encrypted_message_is_left_untouched() {
    let store = test_store();
    let source = b"From: peer@x.com\r\nTo: me@work.local\r\nSubject: hi\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\r\nThe words here.\r\n"
        .to_vec();
    let app = app_with_source(store.clone(), source, "Quarterly report").await;

    let reading = open(&app).await;

    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains("The words here.")),
        "the store's own body renders: {:?}",
        reading.plain
    );
    assert_eq!(reading.security_banner, None, "no banner without a verdict");
    assert_eq!(reading.subject, "Quarterly report");
    assert!(read_result(&store, "m1").is_none(), "no result row lands");
}

/// A scripted decoder answering a closed crypto verdict, the seam's
/// fail-closed contract under test without the crypto machinery.
struct ScriptedVerdict;

#[async_trait::async_trait]
impl MessageDecoder for ScriptedVerdict {
    async fn decode(&self, _message: &MessageRef, _raw: &[u8]) -> Option<DecodeOutcome> {
        Some(DecodeOutcome {
            banner: Some(SecurityBannerView {
                level: SecurityLevelView::Error,
                text: FAILED.to_owned(),
            }),
            crypto_verdict: true,
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn a_crypto_verdict_clears_the_store_own_legs() {
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let source = b"Content-Type: text/plain; charset=utf-8\r\n\r\nThe words here.\r\n".to_vec();
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::mail("m1", "inbox", "Quarterly report")],
        vec![("m1".to_owned(), source)],
    );
    let decoder: Arc<dyn MessageDecoder> = Arc::new(ScriptedVerdict);
    let app = fakes::app(vec![fakes::account(RECIPIENT, provider)], &surfaces)
        .with_decoder(Some(decoder));
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let reading = open(&app).await;

    assert_eq!(
        reading.plain, None,
        "the store's own words never render beside a closed verdict"
    );
    assert_eq!(reading.html, None);
    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Error,
            text: FAILED.to_owned(),
        })
    );
    assert_eq!(
        reading.subject, "Quarterly report",
        "the envelope subject stands"
    );
}
