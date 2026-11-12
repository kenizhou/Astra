// SPDX-License-Identifier: GPL-3.0-only
//! The built-in crypto decoder's S/MIME proofs through the reading pipeline:
//! a scripted S/MIME `ReceiveOutcome` for each state maps to the right banner
//! level and words, the no-key / failed closed arms reuse the one closed
//! banner, and a real sign-then-encrypt S/MIME source decrypts through the
//! unified `receive` dispatch to the words with the chain-verified banner and
//! the restored protected subject.
//!
//! The fixtures use the smime crate's `testing` builders (a real root →
//! intermediate → sender leaf chain) and the production `SmimeBackend::encrypt`
//! seam, the receive orchestrator's test pattern. The app harness is the decode
//! seam's own: `FakeProvider::with_sources` serving the raw RFC5322, one
//! `Intent::OpenMessage`, the snapshot read back.

use std::sync::{Arc, Mutex};

use astra_crypto_core::{
    mime, CryptoBackend, CryptoPolicy, EncryptOp, Fingerprint, KeyHandle, KeyHandleRef, KeyId,
    KeyUsage, Part, PartId, PartKind, SerializationStrategy, SignatureState, Standard,
};
use astra_crypto_smime::testing::{
    build_intermediate_signed_by, build_leaf_signed_by, build_self_signed_ca,
    build_signed_data_with_certs, BuiltCert,
};
use astra_crypto_smime::{fingerprint_of_cert_der, SmimeBackend};
use astra_crypto_store::{
    put_trust_decision, upsert_crypto_key, upsert_intermediate_cert, CryptoKeyRecord, CryptoKeyRow,
    CryptoStore, ReceiveOutcome, ReceiveState, SqliteKeyStore,
};
use astra_viewmodel::{ReadingSnapshot, SecurityLevelView};

use crate::{
    crypto_decode::CryptoDecoder, dispatch_fakes as fakes, App, Intent, MessageDecoder, MessageRef,
    Surface,
};

/// The sender's account in the fixture store; the recipient is the app's
/// `work` account, the one the reading pipeline resolves.
const SENDER: &str = "peer";
const RECIPIENT: &str = "work";

/// The verified banner the decrypted arm carries.
const VERIFIED: &str = "Encrypted message; signature verified.";
/// The unverified banner: the chain validated but no trust decision stands.
const UNVERIFIED: &str = "Encrypted message; signature unverified.";
/// The mismatch banner: the From address does not match the signer cert SAN.
const MISMATCH: &str = "Encrypted message; From address does not match the signer certificate.";
/// The invalid banner: the chain did not validate.
const INVALID: &str = "Encrypted message; signature invalid.";
/// The revoked banner: the chain hard-failed on a revocation.
const REVOKED: &str = "Encrypted message; signature invalid (certificate revoked).";
/// The no-key banner (fail closed).
const NO_KEY: &str = "Encrypted message; cannot decrypt (no key).";
/// The failed-decrypt banner (fail closed).
const FAILED: &str = "Encrypted message; cannot decrypt.";

/// A scripted S/MIME decrypted outcome carrying `state` and `revocation`.
fn smime_outcome(state: SignatureState, revocation: &str) -> ReceiveOutcome {
    ReceiveOutcome {
        state: ReceiveState::Decrypted,
        signature_state: state,
        signer_fingerprint: None,
        signer_email: None,
        weak_warning: None,
        subject: None,
        plain: None,
        html: None,
        standard: Standard::Smime,
        chain_valid: Some(true),
        revocation_state: Some(revocation.to_owned()),
        failure_reason: None,
        revocation_reason: None,
    }
}

/// The banner a scripted outcome maps to.
fn banner_of(outcome: &ReceiveOutcome) -> astra_viewmodel::SecurityBannerView {
    super::decrypted(outcome)
        .banner
        .expect("a decrypted outcome carries a banner")
}

// --- The S/MIME banner arms (scripted outcomes through the mapper) ---

#[test]
fn smime_verified_banners_info() {
    let banner = banner_of(&smime_outcome(SignatureState::ValidVerified, "unchecked"));
    assert_eq!(banner.level, SecurityLevelView::Info);
    assert_eq!(banner.text, VERIFIED);
}

#[test]
fn smime_unverified_banners_warning() {
    let banner = banner_of(&smime_outcome(SignatureState::ValidUnverified, "unchecked"));
    assert_eq!(banner.level, SecurityLevelView::Warning);
    assert_eq!(banner.text, UNVERIFIED);
}

#[test]
fn smime_mismatch_banners_warning() {
    let banner = banner_of(&smime_outcome(SignatureState::Mismatch, "unchecked"));
    assert_eq!(banner.level, SecurityLevelView::Warning);
    assert_eq!(banner.text, MISMATCH);
}

#[test]
fn smime_invalid_banners_warning() {
    let banner = banner_of(&smime_outcome(SignatureState::Invalid, "unchecked"));
    assert_eq!(banner.level, SecurityLevelView::Warning);
    assert_eq!(banner.text, INVALID);
}

#[test]
fn smime_revoked_invalid_distinguishes_the_revocation() {
    let banner = banner_of(&smime_outcome(SignatureState::Invalid, "revoked"));
    assert_eq!(banner.level, SecurityLevelView::Warning);
    assert_eq!(banner.text, REVOKED);
}

#[test]
fn smime_not_signed_banners_info() {
    let banner = banner_of(&smime_outcome(SignatureState::NotSigned, "unchecked"));
    assert_eq!(banner.level, SecurityLevelView::Info);
    assert_eq!(banner.text, "Encrypted message; not signed.");
}

// --- The closed arms (fail closed, standard-agnostic) ---

#[test]
fn the_no_key_arm_reuses_the_closed_banner() {
    let outcome = super::closed(NO_KEY);
    let banner = outcome.banner.expect("a closed outcome carries a banner");
    assert_eq!(banner.level, SecurityLevelView::Error);
    assert_eq!(banner.text, NO_KEY);
    assert!(
        outcome.crypto_verdict,
        "a closed answer owns the whole body"
    );
    assert_eq!(outcome.plain, None, "no key never yields plaintext");
    assert_eq!(outcome.html, None, "no key never yields ciphertext");
}

#[test]
fn the_failed_arm_reuses_the_closed_banner() {
    let outcome = super::closed(FAILED);
    let banner = outcome.banner.expect("a closed outcome carries a banner");
    assert_eq!(banner.level, SecurityLevelView::Error);
    assert_eq!(banner.text, FAILED);
    assert!(outcome.crypto_verdict);
    assert_eq!(outcome.plain, None);
    assert_eq!(outcome.html, None);
}

// --- The unified dispatch: a real S/MIME source through the reading pipeline ---

fn test_store() -> Arc<CryptoStore> {
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account(SENDER, "peer@x.com").unwrap();
    store.register_account(RECIPIENT, "me@work.local").unwrap();
    Arc::new(store)
}

fn smime_backend(store: &Arc<CryptoStore>, account: &str) -> SmimeBackend {
    SmimeBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account)),
        CryptoPolicy::default(),
    )
}

/// Wrap DER as a single PEM block (base64 body, 64-col wrapped).
fn pem_block(label: &str, der: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap());
        out.push('\n');
    }
    out.push_str("-----END ");
    out.push_str(label);
    out.push_str("-----\n");
    out
}

/// Import a cert + private key (PEM bundle) into an account, returning its
/// canonical handle. Mirrors the p12/PEM import the key manager performs.
async fn import_identity(
    store: &Arc<CryptoStore>,
    account: &str,
    cert: &BuiltCert,
) -> KeyHandleRef {
    let pem = format!(
        "{}{}",
        pem_block("CERTIFICATE", &cert.cert_der),
        pem_block("PRIVATE KEY", &cert.priv_pkcs8_der),
    );
    smime_backend(store, account)
        .import_key(pem.as_bytes(), None)
        .await
        .expect("import identity")
}

/// Insert a public-only cert row (a contact / trust anchor).
fn insert_public(
    store: &Arc<CryptoStore>,
    account: &str,
    email: Option<&str>,
    der: &[u8],
    origin: &str,
) {
    let fp = fingerprint_of_cert_der(der).unwrap();
    upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: account.to_string(),
                standard: "smime".to_string(),
                key_type: "cert".to_string(),
                email: email.map(str::to_string),
                fingerprint: fp,
                origin: origin.to_string(),
                ..Default::default()
            },
            public_data: hex::encode(der),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
}

/// The decrypted inner entity: the plain leg under the protected real subject.
const PLAIN_ENTITY: &[u8] = b"Content-Type: text/plain; charset=utf-8\r\n\r\nthe secret words";

/// Build the full RFC5322 source the reading pipeline hands the decoder, plus
/// the sender leaf fingerprint. The sign-then-encrypt plaintext is an opaque
/// SignedData embedding the leaf + intermediate, the shape the send gate emits.
async fn smime_fixture() -> (Arc<CryptoStore>, Vec<u8>, String) {
    let store = test_store();
    let root = build_self_signed_ca("CN=Root");
    let inter = build_intermediate_signed_by("CN=Inter", &root);
    let leaf = build_leaf_signed_by("peer@x.com", &inter);
    let recipient = build_leaf_signed_by("me@work.local", &root);
    let leaf_fp = fingerprint_of_cert_der(&leaf.cert_der).unwrap();

    // The recipient's decrypt identity + verify inputs (root anchor, stored
    // intermediate, a verified decision on the sender leaf).
    import_identity(&store, RECIPIENT, &recipient).await;
    insert_public(&store, RECIPIENT, None, &root.cert_der, "imported");
    upsert_intermediate_cert(&store, RECIPIENT, &inter.cert_der).unwrap();
    put_trust_decision(
        &store,
        RECIPIENT,
        "peer@x.com",
        "smime",
        &leaf_fp,
        "verified",
        None,
    )
    .unwrap();

    // The sender's signing identity + the recipient public cert it encrypts to.
    import_identity(&store, SENDER, &leaf).await;
    insert_public(
        &store,
        SENDER,
        Some("me@work.local"),
        &recipient.cert_der,
        "contact",
    );
    insert_public(&store, SENDER, None, &root.cert_der, "imported");
    let recip_fp = fingerprint_of_cert_der(&recipient.cert_der).unwrap();
    put_trust_decision(
        &store,
        SENDER,
        "me@work.local",
        "smime",
        &recip_fp,
        "verified",
        None,
    )
    .unwrap();

    // Sign-then-encrypt through the production seam (peer → work's cert).
    let protected =
        mime::wrap_protected_headers(PLAIN_ENTITY, Some("Subject: the real subject"), true);
    let signed = build_signed_data_with_certs(
        &protected,
        &leaf.cert_der,
        &leaf.priv_pkcs8_der,
        std::slice::from_ref(&inter.cert_der),
    );
    let recip_handle = KeyHandleRef {
        handle: KeyHandle::Software(KeyId(format!("smime|{recip_fp}"))),
        standard: Standard::Smime,
        fingerprint: Fingerprint::new(recip_fp),
        usage: KeyUsage::SignAndEncrypt,
        algorithm: "ECDSA-P256".into(),
    };
    let env = smime_backend(&store, SENDER)
        .encrypt(EncryptOp {
            parts: &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: signed,
            }],
            serialization: SerializationStrategy::SingleMimeBlob,
            recipients: std::slice::from_ref(&recip_handle),
            sign_with: None,
        })
        .await
        .expect("encrypt fixture");
    let entity = mime::wrap_enveloped(&env.parts[0].ciphertext, "enveloped-data");

    let mut source = b"From: peer@x.com\r\nTo: me@work.local\r\nSubject: ...\r\n".to_vec();
    source.extend_from_slice(&entity);
    (store, source, leaf_fp)
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

#[tokio::test]
async fn an_smime_message_decrypts_through_the_unified_receive_dispatch() {
    let (store, source, leaf_fp) = smime_fixture().await;
    let app = app_with_source(store.clone(), source, "...").await;

    let reading = open(&app).await;

    assert_eq!(reading.plain.as_deref(), Some("the secret words"));
    assert_eq!(
        reading.subject, "the real subject",
        "the protected subject restores"
    );
    let banner = reading
        .security_banner
        .as_ref()
        .expect("the chain-verified banner rides the snapshot");
    assert_eq!(banner.level, SecurityLevelView::Info);
    assert_eq!(banner.text, VERIFIED);
    assert!(
        !reading
            .attachments
            .iter()
            .any(|chip| chip.file_name.eq_ignore_ascii_case("smime.p7m")),
        "the ciphertext chip hides: {:?}",
        reading.attachments
    );

    let row = astra_crypto_store::get_message_crypto_result(&store, RECIPIENT, "m1")
        .expect("the result row lands keyed by account and key");
    assert_eq!(row.crypto_kind, "encrypted-signed");
    assert_eq!(row.decrypt_state, "ok");
    assert_eq!(row.signature_state, "valid-verified");
    assert_eq!(row.standard.as_deref(), Some("smime"));
    assert_eq!(row.chain_valid, Some(true));
    assert_eq!(row.signer_fingerprint.as_deref(), Some(leaf_fp.as_str()));
}
