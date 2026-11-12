// SPDX-License-Identifier: GPL-3.0-only
//! The receive orchestrator's proofs: the PGP/MIME unwrap round-trip, the
//! protected-subject restore, key resolution, the signature-state mapping
//! (verified / unknown-signer / tampered / no-key), the result-row linkage,
//! and the fail-closed floor (no key or a failure never yields plaintext).
//!
//! The send half's own primitives build the fixtures: a sign-then-encrypt
//! envelope wrapped by `pgp_mime::wrap_encrypted`, exactly the shape the
//! product send path emits, so the receive path is proved against the real
//! wire framing rather than a hand-rolled stand-in.

use std::sync::Arc;

use astra_crypto_core::{
    mime, pgp_mime, CryptoBackend, CryptoPolicy, EncryptOp, KeyGenParams, KeyHandleRef, Part,
    PartId, PartKind, SerializationStrategy, SignatureState, Standard,
};
use astra_crypto_openpgp::{engine, keymap, OpenpgpBackend};

use super::*;
use crate::{CryptoKeyRecord, CryptoKeyRow, CryptoStore, SqliteKeyStore};

fn test_store() -> Arc<CryptoStore> {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
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
    crate::set_default_signing_key(store, account, "openpgp", handle.fingerprint.as_str()).unwrap();
    handle
}

/// The stored public blob of an account's key, for cross-account import.
fn public_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let record = crate::get_crypto_key_full(store, account, "openpgp", fingerprint)
        .unwrap()
        .expect("the key row exists");
    hex::decode(record.public_data).expect("public blob hex")
}

/// Imports a public cert into an account as a contact (no private material).
fn import_contact(store: &Arc<CryptoStore>, account: &str, email: &str, blob: &[u8]) {
    let certs = keymap::parse_certs(blob).expect("parse contact cert");
    let fp = keymap::fingerprint_of(&certs[0]).as_str().to_string();
    crate::upsert_crypto_key(
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

/// Builds the full RFC5322 source the reading pipeline hands the orchestrator:
/// outer headers (Subject placeholder when protected) plus the PGP/MIME
/// `multipart/encrypted` entity framing a sign-then-encrypt envelope.
async fn encrypt_source(
    store: &Arc<CryptoStore>,
    sender: &str,
    recipient: &KeyHandleRef,
    signer: Option<&KeyHandleRef>,
    inner_entity: &[u8],
    outer_subject: &str,
) -> Vec<u8> {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), sender)),
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
        format!("From: a1@x.com\r\nTo: a2@x.com\r\nSubject: {outer_subject}\r\n").into_bytes();
    source.extend_from_slice(&entity);
    source
}

const PLAIN_ENTITY: &[u8] = b"Content-Type: text/plain; charset=utf-8\r\n\r\nthe secret words";

/// The persisted result row's verdict columns.
struct ResultRow {
    kind: String,
    decrypt: String,
    signature: String,
    fingerprint: Option<String>,
    email: Option<String>,
}

/// The result row for `(account, message)`, or `None` when no row landed.
fn read_result(store: &Arc<CryptoStore>, account: &str, message: &str) -> Option<ResultRow> {
    store
        .conn()
        .query_row(
            "SELECT crypto_kind, decrypt_state, signature_state, signer_fingerprint, signer_email
             FROM message_crypto_results WHERE account_id = ?1 AND message_id = ?2",
            rusqlite::params![account, message],
            |row| {
                Ok(ResultRow {
                    kind: row.get(0)?,
                    decrypt: row.get(1)?,
                    signature: row.get(2)?,
                    fingerprint: row.get(3)?,
                    email: row.get(4)?,
                })
            },
        )
        .ok()
}

/// Corrupts one base64 character deep in the ciphertext body so the SEIPD
/// integrity check fails on decrypt. The midpoint is used: the leading bytes
/// are the PKESK key-id hint, which is not integrity-protected and does not
/// affect decryption, so a tamper there would not fail closed.
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

/// The two-account fixture: a1 sends, a2 receives. `verify` imports a1's
/// public cert into a2 so the embedded signature verifies; otherwise a2 holds
/// no signer cert and the signature degrades to unknown-key.
async fn two_accounts(verify: bool) -> (Arc<CryptoStore>, KeyHandleRef, KeyHandleRef) {
    let store = test_store();
    let a1 = seed_own(&store, "a1", "a1@x.com").await;
    let a2 = seed_own(&store, "a2", "a2@x.com").await;
    import_contact(
        &store,
        "a1",
        "a2@x.com",
        &public_blob(&store, "a2", a2.fingerprint.as_str()),
    );
    if verify {
        import_contact(
            &store,
            "a2",
            "a1@x.com",
            &public_blob(&store, "a1", a1.fingerprint.as_str()),
        );
    }
    (store, a1, a2)
}

// --- The unwrap helpers (the inverses of the send framing) ---

#[test]
fn unwrap_encrypted_round_trips_the_ciphertext() {
    let ciphertext = b"\xc5\x00\x01binary openpgp packets\xff";
    let wrapped = pgp_mime::wrap_encrypted(ciphertext);
    assert_eq!(
        pgp_mime::unwrap_encrypted(&wrapped),
        Some(ciphertext.to_vec()),
        "unwrap recovers the exact ciphertext bytes"
    );
}

#[test]
fn unwrap_encrypted_answers_none_for_other_framing() {
    assert_eq!(
        pgp_mime::unwrap_encrypted(b"Content-Type: text/plain\r\n\r\nhello"),
        None
    );
    // multipart/encrypted with the WRONG protocol is not PGP/MIME.
    let other = b"Content-Type: multipart/encrypted; protocol=\"application/pkcs7-mime\"; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: application/octet-stream\r\n\r\nAAAA\r\n--b--\r\n";
    assert_eq!(pgp_mime::unwrap_encrypted(other), None);
}

#[test]
fn unwrap_protected_headers_restores_the_subject_and_body() {
    let wrapped =
        mime::wrap_protected_headers(PLAIN_ENTITY, Some("Subject: the real subject"), true);
    let (subject, inner) = mime::unwrap_protected_headers(&wrapped).expect("protected framing");
    assert_eq!(subject.as_deref(), Some("the real subject"));
    let (plain, html) = mime::body_legs(&inner);
    assert_eq!(plain.as_deref(), Some("the secret words"));
    assert_eq!(html, None);
}

#[test]
fn unwrap_protected_headers_answers_none_for_a_plain_entity() {
    assert!(mime::unwrap_protected_headers(PLAIN_ENTITY).is_none());
}

#[test]
fn body_legs_splits_multipart_alternative() {
    let entity = b"Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n--alt\r\nContent-Type: text/plain\r\n\r\nthe plain words\r\n--alt\r\nContent-Type: text/html\r\n\r\n<p>the html words</p>\r\n--alt--\r\n";
    let (plain, html) = mime::body_legs(entity);
    assert_eq!(plain.as_deref(), Some("the plain words"));
    assert_eq!(html.as_deref(), Some("<p>the html words</p>"));
}

// --- The orchestrator: the verified arm ---

#[tokio::test]
async fn a_verified_message_decrypts_to_the_words_and_lands_a_result_row() {
    let (store, a1, a2) = two_accounts(true).await;
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp(&store, "a2", "msg1", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::ValidVerified);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(
        out.signer_fingerprint.as_deref(),
        Some(a1.fingerprint.as_str()),
        "the signer resolves to the sender's primary fingerprint"
    );
    assert_eq!(out.signer_email.as_deref(), Some("a1@x.com"));

    let row = read_result(&store, "a2", "msg1").expect("a result row lands");
    assert_eq!(row.kind, "encrypted-signed");
    assert_eq!(row.decrypt, "ok");
    assert_eq!(row.signature, "valid-verified");
    assert_eq!(row.fingerprint.as_deref(), Some(a1.fingerprint.as_str()));
    assert_eq!(row.email.as_deref(), Some("a1@x.com"));
}

#[tokio::test]
async fn the_protected_subject_is_restored() {
    let (store, a1, a2) = two_accounts(true).await;
    let inner = mime::wrap_protected_headers(PLAIN_ENTITY, Some("Subject: the real subject"), true);
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), &inner, "...").await;

    let out = receive_openpgp(&store, "a2", "msg2", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.subject.as_deref(), Some("the real subject"));
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
}

#[tokio::test]
async fn re_receiving_a_message_upserts_the_one_result_row() {
    let (store, a1, a2) = two_accounts(true).await;
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;
    receive_openpgp(&store, "a2", "msg1", &source).await;
    receive_openpgp(&store, "a2", "msg1", &source).await;
    let count: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM message_crypto_results WHERE account_id = 'a2' AND message_id = 'msg1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "the PK (account_id, message_id) holds one row");
}

// --- The orchestrator: the unknown-signer arm ---

#[tokio::test]
async fn an_unknown_signer_degrades_to_unknown_key_but_still_decrypts() {
    let (store, a1, a2) = two_accounts(false).await;
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp(&store, "a2", "msg3", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::UnknownKey);
    assert_eq!(
        out.plain.as_deref(),
        Some("the secret words"),
        "an unknown signer is not a decrypt failure"
    );
    let row = read_result(&store, "a2", "msg3").expect("a row lands");
    assert_eq!(row.decrypt, "ok");
    assert_eq!(row.signature, "unknown-key");
}

// --- The orchestrator: the tampered arm (fail closed) ---

#[tokio::test]
async fn a_tampered_ciphertext_fails_closed_with_no_plaintext() {
    let (store, a1, a2) = two_accounts(true).await;
    let mut source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;
    tamper_ciphertext(&mut source);

    let out = receive_openpgp(&store, "a2", "msg4", &source).await;
    assert_eq!(out.state, ReceiveState::Failed);
    assert_eq!(out.signature_state, SignatureState::Invalid);
    assert!(out.plain.is_none(), "a failure never yields plaintext");
    assert!(out.html.is_none());
    assert!(out.subject.is_none());
    let row = read_result(&store, "a2", "msg4").expect("a row lands");
    assert_eq!(row.decrypt, "failed");
    assert_eq!(row.signature, "invalid");
}

// --- The orchestrator: the no-key arm (fail closed) ---

#[tokio::test]
async fn no_private_key_answers_no_key_and_never_yields_plaintext() {
    let store = test_store();
    let a1 = seed_own(&store, "a1", "a1@x.com").await;
    // a2 holds a PUBLIC cert only: a1 can encrypt to it, but a2 cannot decrypt.
    let a2_cert = engine::generate("A Two <a2@x.com>").unwrap();
    let a2_public = keymap::cert_to_public_blob(&a2_cert).unwrap();
    import_contact(&store, "a2", "a2@x.com", &a2_public);
    import_contact(&store, "a1", "a2@x.com", &a2_public);
    let a2_handle = keymap::cert_to_handle(&a2_cert);

    let source = encrypt_source(&store, "a1", &a2_handle, Some(&a1), PLAIN_ENTITY, "...").await;
    let out = receive_openpgp(&store, "a2", "msg5", &source).await;
    assert_eq!(out.state, ReceiveState::NoKey);
    assert!(out.plain.is_none(), "no key never yields plaintext");
    assert!(out.html.is_none());
    let row = read_result(&store, "a2", "msg5").expect("a row lands");
    assert_eq!(row.decrypt, "no-key");
}

// --- The orchestrator: the not-encrypted arm ---

#[tokio::test]
async fn a_non_encrypted_source_is_left_untouched_with_no_row() {
    let store = test_store();
    let source = b"From: a1@x.com\r\nTo: a2@x.com\r\nSubject: hi\r\nContent-Type: text/plain\r\n\r\na plain body";
    let out = receive_openpgp(&store, "a2", "msg6", source).await;
    assert_eq!(out.state, ReceiveState::NotEncrypted);
    assert!(out.plain.is_none());
    assert!(
        read_result(&store, "a2", "msg6").is_none(),
        "a non-encrypted message writes no result row"
    );
}

#[tokio::test]
async fn recognised_but_unextractable_framing_fails_closed_not_untouched() {
    let store = test_store();
    // PGP/MIME framing the orchestrator recognises, but the ciphertext part is
    // missing so nothing extracts. It must fail closed (a failed row, no
    // plaintext), never answer NotEncrypted and leave the ciphertext to render.
    let source = b"From: a1@x.com\r\nTo: a2@x.com\r\nSubject: hi\r\nContent-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n--b--\r\n";
    let out = receive_openpgp(&store, "a2", "msg7", source).await;
    assert_eq!(out.state, ReceiveState::Failed);
    assert!(
        out.plain.is_none(),
        "an unextractable ciphertext never yields plaintext"
    );
    let row = read_result(&store, "a2", "msg7").expect("a recognised message writes a failed row");
    assert_eq!(row.decrypt, "failed");
}
