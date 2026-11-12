// SPDX-License-Identifier: GPL-3.0-only
//! The S/MIME receive orchestrator's proofs: the state ladder (verified /
//! unverified / mismatch / invalid), the fail-closed floor (no key / tamper /
//! unextractable framing never yields plaintext), the result-row chain fields,
//! the signer-leaf harvest, and the protected-subject restore.
//!
//! The fixtures use the smime crate's `testing` builders (a real root →
//! intermediate → leaf chain) and the production `SmimeBackend::encrypt` seam,
//! so the receive path is proved against the real sign-then-encrypt wire shape.

use std::sync::Arc;

use astra_crypto_core::{mime, CryptoBackend, CryptoPolicy, SignatureState, Standard};
use astra_crypto_smime::testing::{
    build_intermediate_signed_by, build_leaf_signed_by, build_self_signed_ca,
    build_signed_data_with_certs, BuiltCert,
};
use astra_crypto_smime::{fingerprint_of_cert_der, SmimeBackend};
use base64::Engine;

use super::receive_smime;
use crate::{
    upsert_crypto_key, upsert_intermediate_cert, CryptoKeyRecord, CryptoKeyRow, CryptoStore,
    ReceiveState, SqliteKeyStore,
};

const PLAIN_ENTITY: &[u8] = b"Content-Type: text/plain; charset=utf-8\r\n\r\nthe secret words";

fn test_store() -> Arc<CryptoStore> {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
    Arc::new(store)
}

fn backend(store: &Arc<CryptoStore>, account: &str) -> SmimeBackend {
    SmimeBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account)),
        CryptoPolicy::default(),
    )
}

/// Wrap DER as a single PEM block (base64 body, 64-col wrapped).
fn pem_block(label: &str, der: &[u8]) -> String {
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
) -> astra_crypto_core::KeyHandleRef {
    let pem = format!(
        "{}{}",
        pem_block("CERTIFICATE", &cert.cert_der),
        pem_block("PRIVATE KEY", &cert.priv_pkcs8_der),
    );
    backend(store, account)
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

/// The CA chain + recipient identity every fixture shares.
struct Chain {
    root: BuiltCert,
    inter: BuiltCert,
    leaf: BuiltCert,
    recipient: BuiltCert,
}

fn build_chain() -> Chain {
    let root = build_self_signed_ca("CN=Root");
    let inter = build_intermediate_signed_by("CN=Inter", &root);
    let leaf = build_leaf_signed_by("a1@x.com", &inter);
    let recipient = build_leaf_signed_by("a2@x.com", &root);
    Chain {
        root,
        inter,
        leaf,
        recipient,
    }
}

/// How a fixture varies the trust / identity / key setup.
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // a test-fixture flag bag, not a domain type
struct Opts {
    /// Import the root as a trust anchor (false → untrusted chain → Invalid).
    anchor: bool,
    /// Record a `verified` trust decision for the sender leaf.
    verified: bool,
    /// The outer From address (≠ leaf SAN → Mismatch).
    from: &'static str,
    /// Seed the recipient's private key (false → NoKey).
    seed_key: bool,
    /// Sign-then-encrypt (true) vs encrypt-only (false).
    sign: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            anchor: true,
            verified: true,
            from: "a1@x.com",
            seed_key: true,
            sign: true,
        }
    }
}

/// Build the full RFC5322 source the reading pipeline hands the orchestrator,
/// plus the sender leaf fingerprint. The sign-then-encrypt plaintext is an
/// opaque SignedData embedding the leaf + intermediate, exactly the shape the
/// send gate emits.
async fn fixture(opts: Opts) -> (Arc<CryptoStore>, Vec<u8>, String) {
    let store = test_store();
    let chain = build_chain();
    let leaf_fp = fingerprint_of_cert_der(&chain.leaf.cert_der).unwrap();

    // The recipient's decryption identity (a2). Public cert always; private
    // key only when seeded (the NoKey arm imports public-only).
    if opts.seed_key {
        import_identity(&store, "a2", &chain.recipient).await;
    } else {
        insert_public(
            &store,
            "a2",
            Some("a2@x.com"),
            &chain.recipient.cert_der,
            "imported",
        );
    }

    // The sender's signing identity (a1) + the recipient public cert a1
    // encrypts to.
    import_identity(&store, "a1", &chain.leaf).await;
    insert_public(
        &store,
        "a1",
        Some("a2@x.com"),
        &chain.recipient.cert_der,
        "contact",
    );

    // a2's verify inputs: the root anchor + the stored intermediate.
    if opts.anchor {
        insert_public(&store, "a2", None, &chain.root.cert_der, "imported");
    }
    upsert_intermediate_cert(&store, "a2", &chain.inter.cert_der).unwrap();
    if opts.verified {
        crate::put_trust_decision(&store, "a2", opts.from, "smime", &leaf_fp, "verified", None)
            .unwrap();
    }

    // The protected inner entity (the real subject + the body leg).
    let inner = mime::wrap_protected_headers(PLAIN_ENTITY, Some("Subject: the real subject"), true);

    // The plaintext: a SignedData (sign-then-encrypt) or the raw entity
    // (encrypt-only). The SignedData embeds the leaf + intermediate so
    // extract_intermediates recovers the chain cert.
    let plaintext = if opts.sign {
        build_signed_data_with_certs(
            &inner,
            &chain.leaf.cert_der,
            &chain.leaf.priv_pkcs8_der,
            std::slice::from_ref(&chain.inter.cert_der),
        )
    } else {
        inner.clone()
    };

    // Encrypt through the production seam (a1 → a2's recipient cert).
    let recip_fp = fingerprint_of_cert_der(&chain.recipient.cert_der).unwrap();
    let recip_handle = astra_crypto_core::KeyHandleRef {
        handle: astra_crypto_core::KeyHandle::Software(astra_crypto_core::KeyId(format!(
            "smime|{recip_fp}"
        ))),
        standard: Standard::Smime,
        fingerprint: astra_crypto_core::Fingerprint::new(recip_fp),
        usage: astra_crypto_core::KeyUsage::SignAndEncrypt,
        algorithm: "ECDSA-P256".into(),
    };
    let env = backend(&store, "a1")
        .encrypt(astra_crypto_core::EncryptOp {
            parts: &[astra_crypto_core::Part {
                id: astra_crypto_core::PartId("body".into()),
                kind: astra_crypto_core::PartKind::Body,
                data: plaintext,
            }],
            serialization: astra_crypto_core::SerializationStrategy::SingleMimeBlob,
            recipients: std::slice::from_ref(&recip_handle),
            sign_with: None,
        })
        .await
        .expect("encrypt fixture");
    let cms_der = env.parts[0].ciphertext.clone();
    let entity = mime::wrap_enveloped(&cms_der, "enveloped-data");

    let mut source =
        format!("From: {}\r\nTo: a2@x.com\r\nSubject: ...\r\n", opts.from).into_bytes();
    source.extend_from_slice(&entity);
    (store, source, leaf_fp)
}

/// The persisted result row's verdict + chain columns.
struct ResultRow {
    kind: String,
    decrypt: String,
    signature: String,
    fingerprint: Option<String>,
    chain_valid: Option<i64>,
    revocation: String,
    standard: Option<String>,
}

fn read_result(store: &Arc<CryptoStore>, account: &str, message: &str) -> Option<ResultRow> {
    store
        .conn()
        .query_row(
            "SELECT crypto_kind, decrypt_state, signature_state, signer_fingerprint,
                    chain_valid, revocation_state, standard
             FROM message_crypto_results WHERE account_id = ?1 AND message_id = ?2",
            rusqlite::params![account, message],
            |row| {
                Ok(ResultRow {
                    kind: row.get(0)?,
                    decrypt: row.get(1)?,
                    signature: row.get(2)?,
                    fingerprint: row.get(3)?,
                    chain_valid: row.get(4)?,
                    revocation: row.get(5)?,
                    standard: row.get(6)?,
                })
            },
        )
        .ok()
}

// --- The state ladder: the verified arm ---

#[tokio::test]
async fn a_trusted_chain_verified_signer_decrypts_to_the_words_and_restores_the_subject() {
    let (store, source, leaf_fp) = fixture(Opts::default()).await;
    let out = receive_smime(&store, "a2", "msg1", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::ValidVerified);
    assert_eq!(out.standard, Standard::Smime);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(out.subject.as_deref(), Some("the real subject"));
    assert_eq!(out.signer_fingerprint.as_deref(), Some(leaf_fp.as_str()));
    assert_eq!(out.chain_valid, Some(true));

    let row = read_result(&store, "a2", "msg1").expect("a result row lands");
    assert_eq!(row.kind, "encrypted-signed");
    assert_eq!(row.decrypt, "ok");
    assert_eq!(row.signature, "valid-verified");
    assert_eq!(row.chain_valid, Some(1));
    assert_eq!(row.revocation, "unchecked");
    assert_eq!(row.standard.as_deref(), Some("smime"));
    assert_eq!(row.fingerprint.as_deref(), Some(leaf_fp.as_str()));
}

// --- The state ladder: the unverified arm ---

#[tokio::test]
async fn a_trusted_chain_without_a_trust_decision_is_valid_unverified() {
    let opts = Opts {
        verified: false,
        ..Default::default()
    };
    let (store, source, _) = fixture(opts).await;
    let out = receive_smime(&store, "a2", "msg2", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::ValidUnverified);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(out.chain_valid, Some(true));
}

// --- The state ladder: the mismatch arm ---

#[tokio::test]
async fn a_from_san_mismatch_is_mismatch_not_invalid() {
    let opts = Opts {
        from: "other@x.com",
        ..Default::default()
    };
    let (store, source, _) = fixture(opts).await;
    let out = receive_smime(&store, "a2", "msg3", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::Mismatch);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(out.chain_valid, Some(true));
}

// --- The state ladder: the invalid arm ---

#[tokio::test]
async fn an_untrusted_anchor_is_invalid_but_still_decrypts() {
    let opts = Opts {
        anchor: false,
        ..Default::default()
    };
    let (store, source, _) = fixture(opts).await;
    let out = receive_smime(&store, "a2", "msg4", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::Invalid);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(out.chain_valid, Some(false));
    let row = read_result(&store, "a2", "msg4").expect("a row lands");
    assert_eq!(row.chain_valid, Some(0));
}

// --- The encrypt-only arm ---

#[tokio::test]
async fn an_encrypt_only_message_is_not_signed() {
    let opts = Opts {
        sign: false,
        ..Default::default()
    };
    let (store, source, _) = fixture(opts).await;
    let out = receive_smime(&store, "a2", "msg5", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::NotSigned);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(out.subject.as_deref(), Some("the real subject"));
    let row = read_result(&store, "a2", "msg5").expect("a row lands");
    assert_eq!(row.kind, "encrypted");
    assert_eq!(row.signature, "not-signed");
    assert_eq!(row.chain_valid, None);
}

// --- The fail-closed floor: no key ---

#[tokio::test]
async fn no_private_key_answers_no_key_and_never_yields_plaintext() {
    let opts = Opts {
        seed_key: false,
        ..Default::default()
    };
    let (store, source, _) = fixture(opts).await;
    let out = receive_smime(&store, "a2", "msg6", &source).await;
    assert_eq!(out.state, ReceiveState::NoKey);
    assert!(out.plain.is_none(), "no key never yields plaintext");
    assert!(out.subject.is_none());
    let row = read_result(&store, "a2", "msg6").expect("a row lands");
    assert_eq!(row.decrypt, "no-key");
}

// --- The fail-closed floor: tampered ciphertext ---

#[tokio::test]
async fn a_tampered_ciphertext_fails_closed_with_no_plaintext() {
    let (store, mut source, _) = fixture(Opts::default()).await;
    // Flip one base64 character deep in the ciphertext body.
    let marker = b"Content-Transfer-Encoding: base64";
    let pos = source
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap();
    let after = pos + marker.len();
    let blank = source[after..]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap();
    let idx = after + blank + 4 + 40;
    source[idx] = if source[idx] == b'A' { b'B' } else { b'A' };

    let out = receive_smime(&store, "a2", "msg7", &source).await;
    assert_eq!(out.state, ReceiveState::Failed);
    assert!(out.plain.is_none(), "a failure never yields plaintext");
    let row = read_result(&store, "a2", "msg7").expect("a row lands");
    assert_eq!(row.decrypt, "failed");
}

// --- The not-encrypted arm ---

#[tokio::test]
async fn a_non_encrypted_source_is_left_untouched_with_no_row() {
    let store = test_store();
    let source = b"From: a1@x.com\r\nTo: a2@x.com\r\nSubject: hi\r\nContent-Type: text/plain\r\n\r\na plain body";
    let out = receive_smime(&store, "a2", "msg8", source).await;
    assert_eq!(out.state, ReceiveState::NotEncrypted);
    assert!(out.plain.is_none());
    assert!(read_result(&store, "a2", "msg8").is_none());
}

// --- The fail-closed floor: recognised but unextractable ---

#[tokio::test]
async fn recognised_but_unextractable_framing_fails_closed() {
    let store = test_store();
    let source = b"From: a1@x.com\r\nTo: a2@x.com\r\nSubject: hi\r\nContent-Type: application/pkcs7-mime; smime-type=enveloped-data\r\nContent-Transfer-Encoding: base64\r\n\r\n!!!not base64!!!";
    let out = receive_smime(&store, "a2", "msg9", source).await;
    assert_eq!(out.state, ReceiveState::Failed);
    assert!(out.plain.is_none());
    let row = read_result(&store, "a2", "msg9").expect("a recognised message writes a failed row");
    assert_eq!(row.decrypt, "failed");
}

// --- The signer-leaf harvest (never auto-trusted) ---

#[tokio::test]
async fn the_signer_leaf_is_harvested_but_never_auto_trusted() {
    let (store, source, leaf_fp) = fixture(Opts {
        verified: false,
        ..Default::default()
    })
    .await;
    receive_smime(&store, "a2", "msg10", &source).await;

    // The leaf lands as a harvested public-only row.
    let origin: String = store
        .conn()
        .query_row(
            "SELECT origin FROM crypto_keys WHERE account_id = 'a2' AND standard = 'smime' AND fingerprint = ?1",
            rusqlite::params![leaf_fp],
            |r| r.get(0),
        )
        .expect("the signer leaf is harvested");
    assert_eq!(origin, "harvested");

    // No trust decision was written for it (harvest never auto-trusts).
    let decisions: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM trust_decisions WHERE account_id = 'a2' AND fingerprint = ?1",
            rusqlite::params![leaf_fp],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(decisions, 0, "a harvested leaf is never auto-trusted");
}

#[test]
fn a_token_row_builds_a_token_handle_not_software() {
    // A token-held key must route to the W6 token path, not be offered as
    // Software (which has no private blob and would abort the candidate
    // fallback before a usable software key is tried).
    let mut row = CryptoKeyRow {
        account_id: "a1".to_string(),
        standard: "smime".to_string(),
        key_type: "cert".to_string(),
        fingerprint: "fp".to_string(),
        ..Default::default()
    };
    let software = super::handle_for_row(&row);
    assert!(
        matches!(software.handle, astra_crypto_core::KeyHandle::Software(_)),
        "a software row builds a Software handle"
    );
    row.token_serial = Some("serial".to_string());
    row.token_key_id = Some("keyid".to_string());
    let token = super::handle_for_row(&row);
    assert!(
        matches!(token.handle, astra_crypto_core::KeyHandle::Token { .. }),
        "a token row builds a Token handle, mirroring the OpenPGP path"
    );
}
