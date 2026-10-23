// SPDX-License-Identifier: GPL-3.0-only
//! The seam wrapper's proofs: the moved helpers (Bcc strip, gate words,
//! address slice), the factory's fail-closed arms, and an end-to-end
//! OpenPGP wrap where wire and filed differ only in the `Bcc` header.

use engine_core::ids::MessageIdHeader;
use engine_core::mail::EmailAddress;
use engine_provider::MimeWrapper;

use astra_crypto_core::{CryptoBackend, CryptoMethod, KeyGenParams, RecipientTrustState, Standard};

use super::*;

fn test_store() -> Arc<CryptoStore> {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    Arc::new(store)
}

fn engine_draft(to: &[&str], bcc: &[&str]) -> Draft {
    let mut draft = Draft::new(
        MessageIdHeader::new("wrap-test@x.com").expect("a message id"),
        EmailAddress::new("a1@x.com"),
        to.iter().map(|e| EmailAddress::new(*e)).collect(),
        "the subject",
        "the body",
    )
    .with_crypto(true, true);
    if !bcc.is_empty() {
        draft = draft.with_bcc(bcc.iter().map(|e| EmailAddress::new(*e)).collect());
    }
    draft
}

/// The filed assembly the engine would hand over: headers, then a blank
/// line, then the body.
const FILED: &[u8] = b"From: a1@x.com\r\nTo: bob@x.com\r\nBcc: carol@x.com\r\nSubject: the subject\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nthe body";

// --- The moved helpers ---

#[test]
fn strip_bcc_drops_the_header_and_its_folded_continuations() {
    let wrapped = b"To: bob@x.com\r\nBcc: carol@x.com,\r\n dave@x.com\r\nSubject: s\r\n\r\nbody";
    let out = strip_bcc_header(wrapped);
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("Bcc"), "{text}");
    assert!(
        !text.contains("dave@x.com"),
        "the continuation goes too: {text}"
    );
    assert!(
        text.contains("To: bob@x.com\r\nSubject: s\r\n\r\nbody"),
        "{text}"
    );
}

#[test]
fn strip_bcc_keeps_a_body_line_that_starts_with_bcc() {
    let wrapped = b"Bcc: carol@x.com\r\nSubject: s\r\n\r\nBcc: this is content, not a header";
    let out = strip_bcc_header(wrapped);
    let text = String::from_utf8(out).unwrap();
    assert!(!text.starts_with("Bcc"), "{text}");
    assert!(
        text.contains("\r\n\r\nBcc: this is content, not a header"),
        "below the blank line it is content: {text}"
    );
}

#[test]
fn strip_bcc_passes_through_a_message_without_one() {
    let wrapped = b"To: bob@x.com\r\n\r\nbody";
    assert_eq!(strip_bcc_header(wrapped), wrapped);
    // And non-UTF-8 ciphertext is never touched.
    let binary = b"\xff\xfe\x00binary";
    assert_eq!(strip_bcc_header(binary), binary);
}

#[test]
fn gate_failure_words_names_each_recipient_and_state() {
    let failure = CryptoSendError::RecipientTrust(vec![
        astra_crypto_core::RecipientTrustFailure {
            email: "bob@x.com".to_string(),
            state: RecipientTrustState::NoKey,
            fingerprint: None,
        },
        astra_crypto_core::RecipientTrustFailure {
            email: "carol@x.com".to_string(),
            state: RecipientTrustState::Undecided,
            fingerprint: None,
        },
    ]);
    let words = gate_failure_words(&failure);
    assert!(
        words.starts_with("not every recipient can be encrypted to: "),
        "{words}"
    );
    assert!(words.contains("bob@x.com (no-key)"), "{words}");
    assert!(words.contains("carol@x.com (undecided)"), "{words}");
}

#[test]
fn address_spec_carries_the_display_name_and_address() {
    let spec = address_spec(&EmailAddress {
        name: Some("Bob Peer".to_string()),
        email: "bob@x.com".to_string(),
    });
    assert_eq!(spec.name.as_deref(), Some("Bob Peer"));
    assert_eq!(spec.email, "bob@x.com");
}

// --- The factory's arms ---

#[test]
fn an_account_without_a_method_installs_no_wrapper() {
    let store = test_store();
    assert!(mime_wrapper_for(&store, "a1", "a1@x.com", CryptoMethod::None).is_none());
}

#[tokio::test]
async fn an_unbuildable_wrapper_refuses_rather_than_falls_back_to_plaintext() {
    let refusing = AlwaysRefusing("the store is gone".to_string());
    let err = refusing
        .wrap(&engine_draft(&["bob@x.com"], &[]), FILED.to_vec())
        .await
        .expect_err("fail-closed: a refusal, never a plaintext send");
    assert!(err.to_string().contains("the store is gone"), "{err}");
}

// --- The end-to-end wrap ---

/// Seeds the account's own OpenPGP key (default signer + encrypt-to-self)
/// and a verified recipient key for `email`.
async fn seed_openpgp(store: &Arc<CryptoStore>, email: &str) {
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), "a1")),
        CryptoPolicy::default(),
    );
    let own = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "A One <a1@x.com>".to_string(),
            algorithm: "default".to_string(),
            passphrase: None,
        })
        .await
        .expect("generate own pgp key");
    crate::set_default_signing_key(store, "a1", "openpgp", own.fingerprint.as_str()).unwrap();

    let cert = astra_crypto_openpgp::engine::generate(&format!("Peer <{email}>")).unwrap();
    let fp = astra_crypto_openpgp::keymap::fingerprint_of(&cert)
        .as_str()
        .to_string();
    crate::upsert_crypto_key(
        store,
        &crate::CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "openpgp".to_string(),
                key_type: "cert".to_string(),
                email: Some(email.to_string()),
                fingerprint: fp.clone(),
                origin: "contact".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(
                astra_crypto_openpgp::keymap::cert_to_public_blob(&cert).unwrap(),
            ),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
    crate::put_trust_decision(store, "a1", email, "openpgp", &fp, "verified", None).unwrap();
}

#[tokio::test]
async fn an_openpgp_send_wraps_once_and_the_variants_differ_only_in_bcc() {
    let store = test_store();
    seed_openpgp(&store, "bob@x.com").await;
    seed_openpgp(&store, "carol@x.com").await;
    let wrapper = mime_wrapper_for(&store, "a1", "a1@x.com", CryptoMethod::Openpgp)
        .expect("a method installs a wrapper");

    let outcome = wrapper
        .wrap(
            &engine_draft(&["bob@x.com"], &["carol@x.com"]),
            FILED.to_vec(),
        )
        .await
        .expect("the gate wraps");

    let filed = String::from_utf8(outcome.filed.clone()).expect("utf-8 filed");
    let wire = String::from_utf8(outcome.wire.clone()).expect("utf-8 wire");
    // The filed variant keeps the blind list; the wire variant hides it.
    assert!(filed.contains("Bcc: carol@x.com\r\n"), "{filed}");
    assert!(!wire.contains("Bcc:"), "{wire}");
    // Neither carries the plaintext body; both carry the sealed body.
    assert!(!filed.contains("\r\n\r\nthe body"), "{filed}");
    assert!(!wire.contains("\r\n\r\nthe body"), "{wire}");
    // One wrap, two variants: stripping the filed copy's Bcc IS the wire copy.
    assert_eq!(
        strip_bcc_header(&outcome.filed),
        outcome.wire,
        "the wire variant is the same ciphertext minus Bcc"
    );
}

#[tokio::test]
async fn a_recipient_without_a_key_refuses_permanently_with_the_trust_words() {
    let store = test_store();
    seed_openpgp(&store, "bob@x.com").await;
    let wrapper = mime_wrapper_for(&store, "a1", "a1@x.com", CryptoMethod::Openpgp)
        .expect("a method installs a wrapper");

    let err = wrapper
        .wrap(&engine_draft(&["stranger@x.com"], &[]), FILED.to_vec())
        .await
        .expect_err("fail-closed on an unkeyed recipient");

    let words = err.to_string();
    assert!(
        words.contains("not every recipient can be encrypted to"),
        "{words}"
    );
    assert!(words.contains("stranger@x.com"), "{words}");
}
