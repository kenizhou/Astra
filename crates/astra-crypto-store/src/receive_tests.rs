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
    PartId, PartKind, SecretBox, SerializationStrategy, SignatureState, Standard,
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

// --- The unified dispatcher (SP4c-2): sniff the framing, route ---

#[tokio::test]
async fn the_unified_dispatcher_routes_pgp_and_leaves_plain_untouched() {
    let (store, a1, a2) = two_accounts(true).await;
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;
    let out = crate::receive(&store, "a2", "d1", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.standard, Standard::OpenPgp);

    let plain = b"From: a1@x.com\r\nContent-Type: text/plain\r\n\r\nhi";
    let out = crate::receive(&store, "a2", "d2", plain).await;
    assert_eq!(out.state, ReceiveState::NotEncrypted);
}

// --- The Autocrypt consume hooks (SP4c-3a): silent staging on receive ---

/// The sender's minimal-export blob (the bytes an Autocrypt header carries).
fn sender_minimal_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let blob = public_blob(store, account, fingerprint);
    let cert = keymap::parse_certs(&blob)
        .expect("parse the sender cert")
        .remove(0);
    crate::autocrypt::minimal_export(&cert).expect("minimal export")
}

#[tokio::test]
async fn a_cleartext_autocrypt_header_stages_the_sender_key_without_changing_the_verdict() {
    // a2 holds no signer cert for a1, so the embedded signature is unknown at
    // decrypt time. The consume stages a1's key AFTER the verdict is computed.
    let (store, a1, a2) = two_accounts(false).await;
    let minimal = sender_minimal_blob(&store, "a1", a1.fingerprint.as_str());
    let header = crate::autocrypt::emit_autocrypt_header("a1@x.com", &minimal);
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;
    let source = crate::autocrypt::inject_headers(&source, &[header]);

    let out = receive_openpgp(&store, "a2", "ac1", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(
        out.signature_state,
        SignatureState::UnknownKey,
        "staging is silent: this message's verdict is unchanged by the staged key"
    );

    let rows = crate::list_collected_keys_for_account(&store, "a2", "openpgp").unwrap();
    assert_eq!(rows.len(), 1, "the sender key stages once");
    assert_eq!(rows[0].source.as_deref(), Some("autocrypt"));
    assert_eq!(rows[0].peer_email.as_deref(), Some("a1@x.com"));
    assert_eq!(
        rows[0].fingerprint.as_deref(),
        Some(a1.fingerprint.as_str())
    );
}

#[tokio::test]
async fn a_cleartext_autocrypt_header_whose_addr_is_not_from_stages_nothing() {
    let (store, a1, a2) = two_accounts(false).await;
    let minimal = sender_minimal_blob(&store, "a1", a1.fingerprint.as_str());
    // The header advertises a different addr than the From (a1@x.com).
    let header = crate::autocrypt::emit_autocrypt_header("intruder@x.com", &minimal);
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;
    let source = crate::autocrypt::inject_headers(&source, &[header]);

    let out = receive_openpgp(&store, "a2", "ac2", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert!(
        crate::list_collected_keys_for_account(&store, "a2", "openpgp")
            .unwrap()
            .is_empty(),
        "the addr-to-From binding discards the mismatched header"
    );
}

#[tokio::test]
async fn gossip_headers_inside_the_ciphertext_stage_the_peer_keys() {
    let (store, a1, a2) = two_accounts(false).await;
    let c3 = engine::generate("A Three <a3@x.com>").unwrap();
    let c4 = engine::generate("A Four <a4@x.com>").unwrap();
    let b3 = crate::autocrypt::minimal_export(&c3).unwrap();
    let b4 = crate::autocrypt::minimal_export(&c4).unwrap();
    let fp3 = keymap::fingerprint_of(&c3).as_str().to_string();
    let fp4 = keymap::fingerprint_of(&c4).as_str().to_string();

    // The send gate's layout: the protected-headers wrapper, then the gossip
    // headers injected into its root header block, all inside the ciphertext.
    let wrapped =
        mime::wrap_protected_headers(PLAIN_ENTITY, Some("Subject: the real subject"), true);
    let inner = crate::autocrypt::inject_headers(
        &wrapped,
        &[
            crate::autocrypt::emit_gossip_header("a3@x.com", &b3),
            crate::autocrypt::emit_gossip_header("a4@x.com", &b4),
        ],
    );
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), &inner, "...").await;

    let out = receive_openpgp(&store, "a2", "g1", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(
        out.subject.as_deref(),
        Some("the real subject"),
        "the protected subject still restores beside the gossip consume"
    );

    let rows = crate::list_collected_keys_for_account(&store, "a2", "openpgp").unwrap();
    assert_eq!(rows.len(), 2, "both gossip peer keys stage");
    let mut peers: Vec<String> = rows.iter().map(|r| r.peer_email.clone().unwrap()).collect();
    peers.sort();
    assert_eq!(peers, vec!["a3@x.com".to_string(), "a4@x.com".to_string()]);
    for row in &rows {
        assert_eq!(row.source.as_deref(), Some("autocrypt"));
    }
    let fps: Vec<String> = rows
        .iter()
        .map(|r| r.fingerprint.clone().unwrap())
        .collect();
    assert!(fps.contains(&fp3) && fps.contains(&fp4));
}

#[tokio::test]
async fn a_failed_decrypt_stages_nothing_and_stays_failed() {
    let (store, a1, a2) = two_accounts(false).await;
    let minimal = sender_minimal_blob(&store, "a1", a1.fingerprint.as_str());
    let header = crate::autocrypt::emit_autocrypt_header("a1@x.com", &minimal);
    let source = encrypt_source(&store, "a1", &a2, Some(&a1), PLAIN_ENTITY, "...").await;
    let mut source = crate::autocrypt::inject_headers(&source, &[header]);
    tamper_ciphertext(&mut source);

    let out = receive_openpgp(&store, "a2", "ac3", &source).await;
    assert_eq!(
        out.state,
        ReceiveState::Failed,
        "the fail-closed arm is preserved"
    );
    assert!(out.plain.is_none());
    assert!(
        crate::list_collected_keys_for_account(&store, "a2", "openpgp")
            .unwrap()
            .is_empty(),
        "a failed receive stages nothing"
    );
}

// --- The W9 password flow (SP4c-3c D1): the needs-password verdict + the
//     password orchestrator ---

/// A passphrase as the in-memory secret the orchestrator consumes (never
/// logged, never persisted).
fn passphrase(s: &str) -> SecretBox<String> {
    SecretBox::new(Box::new(s.to_string()))
}

/// The raw SKESK ciphertext for `inner_entity` under `passphrase`, optionally
/// inline-signed by `signer`. The send-side primitive the live proof also uses
/// (W9 send is itself unwired, so the fixture hand-builds the SKESK message).
async fn symmetric_ciphertext(
    store: &Arc<CryptoStore>,
    sender: &str,
    pass: &str,
    signer: Option<&KeyHandleRef>,
    inner_entity: &[u8],
) -> Vec<u8> {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), sender)),
        CryptoPolicy::default(),
    );
    let env = backend
        .encrypt_symmetric(
            &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: inner_entity.to_vec(),
            }],
            SerializationStrategy::SingleMimeBlob,
            passphrase(pass),
            signer,
        )
        .await
        .expect("symmetric encrypt the fixture");
    env.parts[0].ciphertext.clone()
}

/// The full RFC5322 source for a SKESK (passphrase-protected) PGP/MIME message:
/// outer headers plus the `multipart/encrypted` framing a symmetric envelope.
async fn symmetric_source(
    store: &Arc<CryptoStore>,
    sender: &str,
    pass: &str,
    signer: Option<&KeyHandleRef>,
    inner_entity: &[u8],
    outer_subject: &str,
) -> Vec<u8> {
    let ciphertext = symmetric_ciphertext(store, sender, pass, signer, inner_entity).await;
    let entity = pgp_mime::wrap_encrypted(&ciphertext);
    let mut source =
        format!("From: a1@x.com\r\nTo: a2@x.com\r\nSubject: {outer_subject}\r\n").into_bytes();
    source.extend_from_slice(&entity);
    source
}

/// A hybrid ciphertext carrying BOTH a PKESK slot (encrypted to `recipient_cert`)
/// and a SKESK slot (the passphrase), wrapping ONE session key over ONE SEIPD
/// packet. Sequoia's `for_recipients(...).add_passwords(...)` builds exactly this
/// shape: either the recipient's key OR the passphrase opens the same ciphertext.
fn hybrid_ciphertext(recipient_cert: &openpgp::Cert, pass: &str, inner_entity: &[u8]) -> Vec<u8> {
    use openpgp::policy::StandardPolicy;
    use openpgp::serialize::stream::{Encryptor, LiteralWriter, Message};
    use std::io::Write;

    let policy = StandardPolicy::new();
    let recipients = recipient_cert
        .keys()
        .with_policy(&policy, None)
        .supported()
        .alive()
        .revoked(false)
        .for_transport_encryption();
    let mut sink: Vec<u8> = Vec::new();
    let encryptor = Encryptor::for_recipients(Message::new(&mut sink), recipients)
        .add_passwords(Some(pass))
        .build()
        .expect("build the hybrid encryptor");
    let mut literal = LiteralWriter::new(encryptor)
        .build()
        .expect("build the literal writer");
    literal
        .write_all(inner_entity)
        .expect("write the inner entity");
    literal.finalize().expect("finalize the hybrid ciphertext");
    sink
}

/// A hybrid PGP/MIME source carrying BOTH a PKESK slot (encrypted to
/// `recipient`) and a SKESK slot (the passphrase). The key path must win when a
/// fitting key is present; otherwise the verdict is needs-password. The hybrid
/// is unsigned: the test isolates the slot-selection behaviour.
fn hybrid_source(
    store: &Arc<CryptoStore>,
    recipient: &KeyHandleRef,
    pass: &str,
    inner_entity: &[u8],
    outer_subject: &str,
) -> Vec<u8> {
    let blob = public_blob(store, "a2", recipient.fingerprint.as_str());
    let cert = keymap::parse_certs(&blob)
        .expect("parse the recipient cert")
        .remove(0);
    let ciphertext = hybrid_ciphertext(&cert, pass, inner_entity);
    let entity = pgp_mime::wrap_encrypted(&ciphertext);
    let mut source =
        format!("From: a1@x.com\r\nTo: a2@x.com\r\nSubject: {outer_subject}\r\n").into_bytes();
    source.extend_from_slice(&entity);
    source
}

#[tokio::test]
async fn a_skesk_message_with_no_key_answers_needs_password_and_writes_no_row() {
    let store = test_store();
    let a1 = seed_own(&store, "a1", "a1@x.com").await;
    // a2 holds no private key at all: a SKESK message must prompt, not no-key.
    let source = symmetric_source(&store, "a1", "hunter2", Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp(&store, "a2", "pw1", &source).await;
    assert_eq!(out.state, ReceiveState::NeedsPassword);
    assert!(out.plain.is_none(), "needs-password never yields plaintext");
    assert!(out.html.is_none());
    assert!(out.subject.is_none());
    assert!(
        read_result(&store, "a2", "pw1").is_none(),
        "needs-password is transient: no result row is persisted"
    );
}

#[tokio::test]
async fn a_skesk_message_with_an_unfitting_key_answers_needs_password() {
    let (store, a1, _a2) = two_accounts(false).await;
    // a2 holds a private key, but the message is SKESK-only: no PKESK fits, so
    // the verdict is needs-password (not no-key, not failed).
    let source = symmetric_source(&store, "a1", "hunter2", Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp(&store, "a2", "pw2", &source).await;
    assert_eq!(out.state, ReceiveState::NeedsPassword);
    assert!(out.plain.is_none());
    assert!(
        read_result(&store, "a2", "pw2").is_none(),
        "needs-password writes no row even when a key was offered"
    );
}

#[tokio::test]
async fn the_correct_passphrase_decrypts_to_the_words_and_restores_the_subject() {
    let (store, a1, _a2) = two_accounts(true).await;
    let inner = mime::wrap_protected_headers(PLAIN_ENTITY, Some("Subject: the real subject"), true);
    let source = symmetric_source(&store, "a1", "hunter2", Some(&a1), &inner, "...").await;

    let out = receive_openpgp_password(&store, "a2", "pw3", &source, passphrase("hunter2")).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    assert_eq!(out.subject.as_deref(), Some("the real subject"));
    assert_eq!(out.standard, Standard::OpenPgp);

    let row = read_result(&store, "a2", "pw3").expect("success writes an ok row");
    assert_eq!(row.decrypt, "ok");
    assert_eq!(row.kind, "encrypted-signed");
    assert_eq!(row.signature, "valid-verified");
}

#[tokio::test]
async fn an_unsigned_skesk_message_decrypts_to_not_signed() {
    let store = test_store();
    seed_own(&store, "a1", "a1@x.com").await;
    let source = symmetric_source(&store, "a1", "hunter2", None, PLAIN_ENTITY, "...").await;

    let out = receive_openpgp_password(&store, "a2", "pw4", &source, passphrase("hunter2")).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::NotSigned);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    let row = read_result(&store, "a2", "pw4").expect("an ok row lands");
    assert_eq!(row.decrypt, "ok");
    assert_eq!(row.kind, "encrypted");
    assert_eq!(row.signature, "not-signed");
}

#[tokio::test]
async fn a_wrong_passphrase_returns_the_re_prompt_signal_with_no_plaintext() {
    let store = test_store();
    let a1 = seed_own(&store, "a1", "a1@x.com").await;
    let source = symmetric_source(&store, "a1", "hunter2", Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp_password(&store, "a2", "pw5", &source, passphrase("wrong")).await;
    assert_eq!(out.state, ReceiveState::WrongPassword);
    assert!(
        out.plain.is_none(),
        "a wrong passphrase never leaks plaintext"
    );
    assert!(out.html.is_none());
    assert!(out.subject.is_none());
    assert!(
        read_result(&store, "a2", "pw5").is_none(),
        "the re-prompt signal is transient: no result row is persisted"
    );

    // The same message still decrypts with the correct passphrase afterwards.
    let ok = receive_openpgp_password(&store, "a2", "pw5", &source, passphrase("hunter2")).await;
    assert_eq!(ok.state, ReceiveState::Decrypted);
    assert_eq!(ok.plain.as_deref(), Some("the secret words"));
    assert_eq!(
        read_result(&store, "a2", "pw5")
            .expect("the retry writes an ok row")
            .decrypt,
        "ok"
    );
}

#[tokio::test]
async fn a_password_message_with_a_known_signer_verifies_the_embedded_signature() {
    let (store, a1, _a2) = two_accounts(true).await;
    let source = symmetric_source(&store, "a1", "hunter2", Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp_password(&store, "a2", "pw6", &source, passphrase("hunter2")).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::ValidVerified);
    assert_eq!(
        out.signer_fingerprint.as_deref(),
        Some(a1.fingerprint.as_str()),
        "the embedded signature resolves to the sender's primary fingerprint"
    );
    assert_eq!(out.signer_email.as_deref(), Some("a1@x.com"));
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
}

#[tokio::test]
async fn a_password_message_with_an_unknown_signer_degrades_but_still_decrypts() {
    let (store, a1, _a2) = two_accounts(false).await;
    let source = symmetric_source(&store, "a1", "hunter2", Some(&a1), PLAIN_ENTITY, "...").await;

    let out = receive_openpgp_password(&store, "a2", "pw7", &source, passphrase("hunter2")).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.signature_state, SignatureState::UnknownKey);
    assert_eq!(
        out.plain.as_deref(),
        Some("the secret words"),
        "an unknown signer is not a decrypt failure"
    );
    let row = read_result(&store, "a2", "pw7").expect("an ok row lands");
    assert_eq!(row.decrypt, "ok");
    assert_eq!(row.signature, "unknown-key");
}

#[tokio::test]
async fn a_pkesk_plus_skesk_message_with_a_fitting_key_uses_the_key_path() {
    let (store, _a1, a2) = two_accounts(true).await;
    let source = hybrid_source(&store, &a2, "hunter2", PLAIN_ENTITY, "...");

    // The key path wins: no passphrase is offered, yet the message decrypts.
    let out = receive_openpgp(&store, "a2", "pw8", &source).await;
    assert_eq!(out.state, ReceiveState::Decrypted);
    assert_eq!(out.plain.as_deref(), Some("the secret words"));
    let row = read_result(&store, "a2", "pw8").expect("the key path writes an ok row");
    assert_eq!(row.decrypt, "ok");
}

#[tokio::test]
async fn a_pkesk_plus_skesk_message_with_no_fitting_key_answers_needs_password() {
    let store = test_store();
    // a2 holds a PUBLIC cert only: the PKESK slot cannot be decrypted, so the
    // SKESK slot drives the needs-password verdict.
    let a2_cert = engine::generate("A Two <a2@x.com>").unwrap();
    let a2_public = keymap::cert_to_public_blob(&a2_cert).unwrap();
    import_contact(&store, "a2", "a2@x.com", &a2_public);
    let a2_handle = keymap::cert_to_handle(&a2_cert);

    let source = hybrid_source(&store, &a2_handle, "hunter2", PLAIN_ENTITY, "...");
    let out = receive_openpgp(&store, "a2", "pw9", &source).await;
    assert_eq!(out.state, ReceiveState::NeedsPassword);
    assert!(out.plain.is_none());
    assert!(
        read_result(&store, "a2", "pw9").is_none(),
        "needs-password writes no row"
    );

    // The passphrase still opens the SKESK slot.
    let ok = receive_openpgp_password(&store, "a2", "pw9", &source, passphrase("hunter2")).await;
    assert_eq!(ok.state, ReceiveState::Decrypted);
    assert_eq!(ok.plain.as_deref(), Some("the secret words"));
}

#[tokio::test]
async fn the_password_entry_leaves_a_non_encrypted_source_untouched() {
    let store = test_store();
    let source = b"From: a1@x.com\r\nTo: a2@x.com\r\nSubject: hi\r\nContent-Type: text/plain\r\n\r\na plain body";
    let out = receive_openpgp_password(&store, "a2", "pw10", source, passphrase("hunter2")).await;
    assert_eq!(out.state, ReceiveState::NotEncrypted);
    assert!(out.plain.is_none());
    assert!(
        read_result(&store, "a2", "pw10").is_none(),
        "a non-encrypted message writes no result row"
    );
}

#[tokio::test]
async fn the_password_entry_fails_closed_on_unextractable_framing() {
    let store = test_store();
    let source = b"From: a1@x.com\r\nTo: a2@x.com\r\nSubject: hi\r\nContent-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n--b--\r\n";
    let out = receive_openpgp_password(&store, "a2", "pw11", source, passphrase("hunter2")).await;
    assert_eq!(out.state, ReceiveState::Failed);
    assert!(out.plain.is_none());
    let row = read_result(&store, "a2", "pw11").expect("a recognised message writes a failed row");
    assert_eq!(row.decrypt, "failed");
}
