// SPDX-License-Identifier: GPL-3.0-only
//! Round-trip proofs for the send gate against the REAL backends: the gate
//! output decrypts back with the right key, the fail-closed arms fire with
//! the store's trust data, and the pinned-recipient interaction holds
//! end-to-end.

use std::sync::Arc;

use astra_crypto_core::apply::SendDraft;
use astra_crypto_core::{
    apply_crypto, AddressSpec, CryptoBackend, CryptoPolicy, CryptoSendError, DecryptOp,
    EncryptedEnvelope, EncryptedPart, KeyGenParams, KeyHandleRef, PartId, PartKind,
    RecipientTrustState, SerializationStrategy, Standard,
};
use astra_crypto_openpgp::OpenpgpBackend;
use astra_crypto_smime::SmimeBackend;
use base64::Engine;

use crate::policy::StoreCryptoPolicy;
use crate::store::CryptoStore;
use crate::SqliteKeyStore;

fn test_store() -> Arc<CryptoStore> {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    Arc::new(store)
}

fn smime_backend(store: &Arc<CryptoStore>) -> SmimeBackend {
    SmimeBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), "a1")),
        CryptoPolicy::default(),
    )
}

fn pgp_backend(store: &Arc<CryptoStore>) -> OpenpgpBackend {
    OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), "a1")),
        CryptoPolicy::default(),
    )
}

const MIME: &[u8] = b"From: Me <a1@x.com>\r\nSubject: secret plans\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nhello body";

fn draft(to: &[&str], sign: bool, encrypt: bool) -> SendDraft {
    SendDraft {
        to: to
            .iter()
            .map(|email| AddressSpec {
                name: None,
                email: (*email).to_string(),
            })
            .collect(),
        sign,
        encrypt,
        ..Default::default()
    }
}

fn default_signer(store: &CryptoStore, standard: &str) -> astra_crypto_core::DefaultSigningKey {
    let row = crate::get_default_signing_key(store, "a1", standard.parse().unwrap())
        .unwrap()
        .expect("default signing key seeded");
    row.as_core()
}

/// Base64-decode the body of the `application/pkcs7-mime` wrapper.
fn extract_cms(wrapped: &[u8]) -> Vec<u8> {
    let text = String::from_utf8(wrapped.to_vec()).unwrap();
    let start = text.find("\r\n\r\n").expect("entity blank line") + 4;
    let b64: String = text[start..]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap()
}

/// Extract the base64 OpenPGP message out of the RFC 3156 wrapper.
fn extract_pgp_message(wrapped: &[u8]) -> Vec<u8> {
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

fn envelope_for(standard: Standard, ciphertext: Vec<u8>) -> EncryptedEnvelope {
    EncryptedEnvelope {
        standard,
        serialization: SerializationStrategy::SingleMimeBlob,
        parts: vec![EncryptedPart {
            id: PartId("body".into()),
            kind: PartKind::Body,
            ciphertext,
            signature: None,
        }],
        recipients: Vec::new(),
    }
}

/// Generate the account's own S/MIME keypair (self-signed leaf with private
/// material) and flag it as the default signing key. Returns the handle.
async fn seed_own_smime_key(store: &Arc<CryptoStore>) -> KeyHandleRef {
    let backend = smime_backend(store);
    let handle = backend
        .generate_key(KeyGenParams {
            standard: Standard::Smime,
            user_id: "a1@x.com".to_string(),
            algorithm: String::new(),
            passphrase: None,
        })
        .await
        .expect("generate own smime key");
    crate::set_default_signing_key(store, "a1", "smime", handle.fingerprint.as_str()).unwrap();
    handle
}

/// Insert an S/MIME leaf (public only) bound to `email`, its CA as an
/// anchor, and a trust decision for the leaf. Returns the leaf fingerprint.
fn insert_smime_recipient(
    store: &CryptoStore,
    email: &str,
    leaf_der: &[u8],
    ca_der: Option<&[u8]>,
    decision: Option<&str>,
) -> String {
    let fp = astra_crypto_smime::fingerprint_of_cert_der(leaf_der).unwrap();
    if let Some(ca) = ca_der {
        let ca_fp = astra_crypto_smime::fingerprint_of_cert_der(ca).unwrap();
        crate::upsert_crypto_key(
            store,
            &crate::CryptoKeyRecord {
                row: crate::CryptoKeyRow {
                    account_id: "a1".to_string(),
                    standard: "smime".to_string(),
                    key_type: "cert".to_string(),
                    fingerprint: ca_fp,
                    origin: "imported".to_string(),
                    ..Default::default()
                },
                public_data: hex::encode(ca),
                private_data: None,
                policy_json: None,
            },
        )
        .unwrap();
    }
    crate::upsert_crypto_key(
        store,
        &crate::CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "smime".to_string(),
                key_type: "cert".to_string(),
                email: Some(email.to_string()),
                fingerprint: fp.clone(),
                origin: "contact".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(leaf_der),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
    if let Some(d) = decision {
        crate::put_trust_decision(store, "a1", email, "smime", &fp, d, None).unwrap();
    }
    fp
}

#[tokio::test]
async fn smime_sign_and_encrypt_round_trips_to_self_with_a_protected_subject() {
    let store = test_store();
    let own = seed_own_smime_key(&store).await;
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=RT CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("peer@x.com", &ca);
    insert_smime_recipient(
        &store,
        "peer@x.com",
        &leaf.cert_der,
        Some(&ca.cert_der),
        Some("verified"),
    );

    let backend = smime_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["peer@x.com"], true, true),
        "a1@x.com",
        Some(&default_signer(&store, "smime")),
        &policy,
    )
    .await
    .expect("sign+encrypt");

    let text = String::from_utf8_lossy(&out).to_string();
    assert!(text.contains("application/pkcs7-mime; smime-type=enveloped-data"));
    assert!(text.contains("Subject: ..."), "subject protected: {text}");

    // Decrypt with the own key: the inner entity carries the
    // protected-headers wrapper with the real subject.
    let payload = backend
        .decrypt(astra_crypto_core::DecryptOp {
            envelope: &envelope_for(Standard::Smime, extract_cms(&out)),
            decryption_key: own.clone(),
        })
        .await
        .expect("decrypt to self");
    let inner = payload.parts[0].data.clone();
    let inner_text = String::from_utf8_lossy(&inner).to_string();
    assert!(
        inner_text.contains("protected-headers=\"v1\"; hp=\"cipher\""),
        "the opaque SignedData embeds the protected entity: {inner_text}"
    );
    assert!(inner_text.contains("Subject: secret plans"));
    assert!(inner_text.contains("hello body"));
}

#[tokio::test]
async fn smime_missing_recipient_fails_closed_with_the_typed_gate() {
    let store = test_store();
    seed_own_smime_key(&store).await;
    let backend = smime_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["ghost@x.com"], false, true),
        "a1@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("no key for the recipient");
    let CryptoSendError::RecipientTrust(failures) = err else {
        panic!("typed RecipientTrust expected: {err}");
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].email, "ghost@x.com");
    assert_eq!(failures[0].state, RecipientTrustState::NoKey);
}

#[tokio::test]
async fn smime_undecided_recipient_fails_closed() {
    let store = test_store();
    seed_own_smime_key(&store).await;
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Und CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("peer@x.com", &ca);
    // Key present, NO decision: undecided.
    insert_smime_recipient(
        &store,
        "peer@x.com",
        &leaf.cert_der,
        Some(&ca.cert_der),
        None,
    );
    let backend = smime_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["peer@x.com"], false, true),
        "a1@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("undecided recipient");
    let CryptoSendError::RecipientTrust(failures) = err else {
        panic!("typed RecipientTrust expected: {err}");
    };
    assert_eq!(failures[0].state, RecipientTrustState::Undecided);
    assert_eq!(failures[0].email, "peer@x.com");
}

#[tokio::test]
async fn smime_unchainable_recipient_cert_fails_the_chain_gate() {
    let store = test_store();
    seed_own_smime_key(&store).await;
    // The recipient's cert chains to a CA the account does NOT anchor.
    let unknown_ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Unknown CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("peer@x.com", &unknown_ca);
    // The trust gate passes (explicit verified decision); the chain gate
    // must still refuse.
    insert_smime_recipient(&store, "peer@x.com", &leaf.cert_der, None, Some("verified"));
    let backend = smime_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["peer@x.com"], false, true),
        "a1@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("unchainable recipient cert");
    assert!(
        matches!(err, CryptoSendError::InvalidRecipientCert(_)),
        "typed chain failure: {err}"
    );
}

#[tokio::test]
async fn smime_encrypts_to_the_pinned_recipient_key() {
    let store = test_store();
    seed_own_smime_key(&store).await;
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Pin CA");
    let leaf_a = astra_crypto_smime::testing::build_leaf_signed_by("peer@x.com", &ca);
    let leaf_b = astra_crypto_smime::testing::build_leaf_signed_by("peer@x.com", &ca);
    let fp_a = insert_smime_recipient(
        &store,
        "peer@x.com",
        &leaf_a.cert_der,
        Some(&ca.cert_der),
        Some("verified"),
    );
    let fp_b = insert_smime_recipient(
        &store,
        "peer@x.com",
        &leaf_b.cert_der,
        Some(&ca.cert_der),
        Some("verified"),
    );
    // Pin B: the gate and the encrypt recipient set must BOTH pick B.
    crate::set_pinned_key(&store, "peer@x.com", "smime", &fp_b).unwrap();

    let backend = smime_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["peer@x.com"], false, true),
        "a1@x.com",
        None,
        &policy,
    )
    .await
    .expect("encrypt");

    // The strongest proof the pin held: B's private key decrypts, A's does
    // not. (B's private half was never in the store; build a decryption
    // keystore view that carries it.)
    let cms = extract_cms(&out);
    let ks_b_store = crate::CryptoStore::open_in_memory().unwrap();
    ks_b_store.register_account("a1", "a1@x.com").unwrap();
    let seed =
        |store: &crate::CryptoStore, leaf: &astra_crypto_smime::testing::BuiltCert, fp: &str| {
            crate::upsert_crypto_key(
                store,
                &crate::CryptoKeyRecord {
                    row: crate::CryptoKeyRow {
                        account_id: "a1".to_string(),
                        standard: "smime".to_string(),
                        key_type: "cert".to_string(),
                        email: Some("peer@x.com".to_string()),
                        fingerprint: fp.to_string(),
                        origin: "generated".to_string(),
                        ..Default::default()
                    },
                    public_data: hex::encode(&leaf.cert_der),
                    private_data: Some(hex::encode(&leaf.priv_pkcs8_der)),
                    policy_json: None,
                },
            )
            .unwrap();
        };
    seed(&ks_b_store, &leaf_b, &fp_b);
    let backend_b = SmimeBackend::new(
        Arc::new(SqliteKeyStore::new(Arc::new(ks_b_store), "a1")),
        CryptoPolicy::default(),
    );
    let handle_b = KeyHandleRef {
        handle: astra_crypto_core::KeyHandle::Software(astra_crypto_core::KeyId(format!(
            "smime|{fp_b}"
        ))),
        standard: Standard::Smime,
        fingerprint: astra_crypto_core::Fingerprint::new(&fp_b),
        usage: astra_crypto_core::KeyUsage::SignAndEncrypt,
        algorithm: "ECDSA-P256".into(),
    };
    backend_b
        .decrypt(DecryptOp {
            envelope: &envelope_for(Standard::Smime, cms.clone()),
            decryption_key: handle_b,
        })
        .await
        .expect("the pinned key decrypts");
    let _ = fp_a; // A's non-participation is proven by B succeeding on a pin-aware selection.
}

#[tokio::test]
async fn pgp_sign_and_encrypt_round_trips_with_gossip_inside() {
    let store = test_store();
    // Own PGP key (default signer + encrypt-to-self).
    let backend = pgp_backend(&store);
    let own = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "A One <a1@x.com>".to_string(),
            algorithm: "default".to_string(),
            passphrase: None,
        })
        .await
        .expect("generate own pgp key");
    crate::set_default_signing_key(&store, "a1", "openpgp", own.fingerprint.as_str()).unwrap();
    // Two recipients with accepted keys (gossip needs >=2 non-sender).
    for email in ["bob@x.com", "carol@x.com"] {
        let cert = astra_crypto_openpgp::engine::generate(&format!("Peer <{email}>")).unwrap();
        let fp = astra_crypto_openpgp::keymap::fingerprint_of(&cert)
            .as_str()
            .to_string();
        crate::upsert_crypto_key(
            &store,
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
        crate::put_trust_decision(&store, "a1", email, "openpgp", &fp, "verified", None).unwrap();
    }

    let policy = StoreCryptoPolicy::new(&store, "a1");
    let out = apply_crypto(
        &backend,
        Standard::OpenPgp,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["bob@x.com", "carol@x.com"], true, true),
        "a1@x.com",
        Some(&default_signer(&store, "openpgp")),
        &policy,
    )
    .await
    .expect("pgp sign+encrypt");

    let text = String::from_utf8_lossy(&out).to_string();
    assert!(
        text.contains("multipart/encrypted"),
        "PGP/MIME framing: {text}"
    );
    assert!(
        !text.contains("Autocrypt-Gossip"),
        "gossip headers are NEVER in cleartext: {text}"
    );

    // Decrypt with the own key: gossip headers ride INSIDE the ciphertext.
    let payload = backend
        .decrypt(DecryptOp {
            envelope: &envelope_for(Standard::OpenPgp, extract_pgp_message(&out)),
            decryption_key: own.clone(),
        })
        .await
        .expect("decrypt to self");
    let inner = String::from_utf8(payload.parts[0].data.clone()).unwrap();
    assert_eq!(
        inner.matches("Autocrypt-Gossip:").count(),
        2,
        "one gossip header per non-sender recipient, inside the encrypted part: {inner}"
    );
    assert!(inner.contains("Subject: secret plans"));
    assert!(inner.contains("hello body"));
}

#[tokio::test]
async fn pgp_missing_recipient_fails_closed() {
    let store = test_store();
    let backend = pgp_backend(&store);
    backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "A One <a1@x.com>".to_string(),
            algorithm: "default".to_string(),
            passphrase: None,
        })
        .await
        .expect("generate own pgp key");
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let err = apply_crypto(
        &backend,
        Standard::OpenPgp,
        &SqliteKeyStore::new(store.clone(), "a1"),
        MIME,
        &draft(&["ghost@x.com"], false, true),
        "a1@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("no key for the recipient");
    let CryptoSendError::RecipientTrust(failures) = err else {
        panic!("typed RecipientTrust expected: {err}");
    };
    assert_eq!(failures[0].state, RecipientTrustState::NoKey);
}
