// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the W9 password mode: the typed fail-closed arms and a real
//! SKESK round trip through the concrete OpenPGP backend.

use std::sync::Arc;

use astra_crypto_core::apply::SendDraft;
use astra_crypto_core::{CryptoPolicy, Standard};
use astra_crypto_core::{
    CryptoSendError, EncryptedEnvelope, EncryptedPart, PartId, PartKind, SecretBox,
    SerializationStrategy,
};

use super::*;
use crate::policy::StoreCryptoPolicy;
use crate::store::CryptoStore;
use base64::Engine as _;

fn test_store() -> Arc<CryptoStore> {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    Arc::new(store)
}

fn backend(store: &Arc<CryptoStore>) -> OpenpgpBackend {
    OpenpgpBackend::new(
        Arc::new(crate::SqliteKeyStore::new(store.clone(), "a1")),
        CryptoPolicy::default(),
    )
}

const MIME: &[u8] = b"From: Me <a1@x.com>\r\nSubject: whisper\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nhello body";

fn passphrase(s: &str) -> SecretBox<String> {
    SecretBox::new(Box::new(s.to_string()))
}

#[tokio::test]
async fn encrypt_and_password_modes_conflict() {
    let store = test_store();
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let draft = SendDraft {
        encrypt: true,
        ..Default::default()
    };
    let err = apply_password_crypto(
        &backend(&store),
        MIME,
        &draft,
        None,
        passphrase("hunter2"),
        &policy,
    )
    .await
    .expect_err("both modes requested");
    assert!(matches!(err, CryptoSendError::PasswordConflict));
}

#[tokio::test]
async fn an_empty_passphrase_is_required_before_any_ciphertext() {
    let store = test_store();
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let err = apply_password_crypto(
        &backend(&store),
        MIME,
        &SendDraft::default(),
        None,
        passphrase(""),
        &policy,
    )
    .await
    .expect_err("empty passphrase");
    assert!(matches!(err, CryptoSendError::PasswordRequired));
}

#[tokio::test]
async fn sign_without_a_default_key_is_typed() {
    let store = test_store();
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let draft = SendDraft {
        sign: true,
        ..Default::default()
    };
    let err = apply_password_crypto(
        &backend(&store),
        MIME,
        &draft,
        None,
        passphrase("hunter2"),
        &policy,
    )
    .await
    .expect_err("no default signing key");
    assert!(matches!(err, CryptoSendError::NoSigningKey));
}

/// Extract the base64-transported OpenPGP message out of the RFC 3156
/// `multipart/encrypted` wrapper (part 2, `encrypted.asc`).
fn extract_wrapped_message(wrapped: &[u8]) -> Vec<u8> {
    let text = String::from_utf8(wrapped.to_vec()).unwrap();
    let marker = "Content-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n";
    let start = text.find(marker).expect("part 2 headers") + marker.len();
    let end = text[start..]
        .find("\r\n------=_astra_pgp_encrypted_0001--")
        .expect("closing boundary")
        + start;
    let b64: String = text[start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap()
}

#[tokio::test]
async fn password_mode_round_trips_and_protects_the_subject() {
    let store = test_store();
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let out = apply_password_crypto(
        &backend(&store),
        MIME,
        &SendDraft::default(),
        None,
        passphrase("hunter2"),
        &policy,
    )
    .await
    .expect("password encrypt");

    let text = String::from_utf8_lossy(&out).to_string();
    assert!(
        text.contains("multipart/encrypted"),
        "RFC 3156 framing: {text}"
    );
    assert!(
        text.contains("Subject: ..."),
        "the outer subject is protected by default: {text}"
    );

    // Decrypt back with the same passphrase: the inner entity carries the
    // protected-headers wrapper with the real subject inside.
    let message = extract_wrapped_message(&out);
    let envelope = EncryptedEnvelope {
        standard: Standard::OpenPgp,
        serialization: SerializationStrategy::SingleMimeBlob,
        parts: vec![EncryptedPart {
            id: PartId("body".into()),
            kind: PartKind::Body,
            ciphertext: message,
            signature: None,
        }],
        recipients: Vec::new(),
    };
    let outcome = backend(&store)
        .decrypt_symmetric(&envelope, passphrase("hunter2"), &[], &[])
        .await
        .expect("decrypt with the passphrase");
    let inner = String::from_utf8(outcome.payload.parts[0].data.clone()).unwrap();
    assert!(inner.contains("protected-headers=\"v1\""), "{inner}");
    assert!(inner.contains("Subject: whisper"));
    assert!(inner.contains("hello body"));

    // A wrong passphrase fails typed.
    let err = backend(&store)
        .decrypt_symmetric(&envelope, passphrase("wrong"), &[], &[])
        .await;
    assert!(err.is_err(), "wrong passphrase never decrypts");
}

#[tokio::test]
async fn password_mode_honours_protect_subject_off() {
    let store = test_store();
    crate::crypto_policy::set_policy_flag(&store, "a1", "protect_subject", false).unwrap();
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let out = apply_password_crypto(
        &backend(&store),
        MIME,
        &SendDraft::default(),
        None,
        passphrase("hunter2"),
        &policy,
    )
    .await
    .expect("password encrypt");
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(text.contains("Subject: whisper\r\n"));
}
