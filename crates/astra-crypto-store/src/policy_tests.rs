// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the store-backed send policy: the recipient-cert chain gate and
//! the Autocrypt-Gossip gating (with real generated keys and the real
//! OpenPGP backend, so export + minimal export run for real).

use std::sync::Arc;

use astra_crypto_core::{CryptoPolicy, KeyHandleRef, Standard};
use astra_crypto_openpgp::OpenpgpBackend;

use super::*;
use crate::keystore::SqliteKeyStore;
use crate::store::CryptoStore;

fn test_store() -> Arc<CryptoStore> {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    Arc::new(store)
}

/// Insert an OpenPGP cert (public blob only) bound to `email`; returns the
/// `find_by_email`-resolvable handle.
fn insert_pgp_contact(store: &CryptoStore, email: &str) -> KeyHandleRef {
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
    astra_crypto_core::KeyHandleRef {
        handle: astra_crypto_core::KeyHandle::Software(astra_crypto_core::KeyId(format!(
            "openpgp|{fp}"
        ))),
        standard: Standard::OpenPgp,
        fingerprint: astra_crypto_core::Fingerprint::new(&fp),
        usage: astra_crypto_core::KeyUsage::SignAndEncrypt,
        algorithm: "default".into(),
    }
}

fn pgp_backend(store: &Arc<CryptoStore>) -> OpenpgpBackend {
    OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), "a1")),
        CryptoPolicy::default(),
    )
}

const ENTITY: &[u8] = b"Content-Type: text/plain; charset=utf-8\r\n\r\nhello body";

#[tokio::test]
async fn gossip_headers_land_for_two_distinct_non_sender_recipients() {
    let store = test_store();
    let backend = pgp_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let h_b = insert_pgp_contact(&store, "bob@x.com");
    let h_c = insert_pgp_contact(&store, "carol@x.com");
    let emails: Vec<String> = ["a1@x.com", "bob@x.com", "carol@x.com"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let handles = vec![
        // Sender first (the gate's parallel-vector layout).
        astra_crypto_core::KeyHandleRef {
            handle: astra_crypto_core::KeyHandle::Software(astra_crypto_core::KeyId(
                "openpgp|fpself".into(),
            )),
            standard: Standard::OpenPgp,
            fingerprint: astra_crypto_core::Fingerprint::new("fpself"),
            usage: astra_crypto_core::KeyUsage::SignAndEncrypt,
            algorithm: "default".into(),
        },
        h_b,
        h_c,
    ];

    let out = policy
        .maybe_prepend_gossip_headers(&backend, ENTITY, "a1@x.com", &emails, &handles)
        .await;
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text.matches("Autocrypt-Gossip:").count(),
        2,
        "one folded header per non-sender recipient: {text}"
    );
    assert!(text.contains("addr=bob@x.com"));
    assert!(text.contains("addr=carol@x.com"));
    assert!(
        !text.contains("addr=a1@x.com"),
        "the sender is never gossiped"
    );
    assert!(
        text.ends_with("hello body"),
        "the headers prepend to the entity's root block, the body is untouched"
    );
}

#[tokio::test]
async fn gossip_requires_two_distinct_non_sender_recipients() {
    let store = test_store();
    let backend = pgp_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let h_b = insert_pgp_contact(&store, "bob@x.com");
    // One non-sender recipient (and a case-insensitive DUPLICATE of it):
    // dedup leaves one, which is below the >=2 bar.
    let emails: Vec<String> = ["a1@x.com", "bob@x.com", "BOB@x.com"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let handles = vec![h_b.clone(), h_b.clone(), h_b];
    let out = policy
        .maybe_prepend_gossip_headers(&backend, ENTITY, "a1@x.com", &emails, &handles)
        .await;
    assert_eq!(out, ENTITY.to_vec(), "below the recipient bar: unchanged");
}

#[tokio::test]
async fn gossip_respects_the_setting_off() {
    let store = test_store();
    crate::crypto_policy::set_policy_flag(&store, "a1", "send_autocrypt_headers", false).unwrap();
    let backend = pgp_backend(&store);
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let h_b = insert_pgp_contact(&store, "bob@x.com");
    let h_c = insert_pgp_contact(&store, "carol@x.com");
    let emails: Vec<String> = ["a1@x.com", "bob@x.com", "carol@x.com"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let handles = vec![h_b.clone(), h_b, h_c];
    let out = policy
        .maybe_prepend_gossip_headers(&backend, ENTITY, "a1@x.com", &emails, &handles)
        .await;
    assert_eq!(out, ENTITY.to_vec(), "the setting gates gossip off");
}

#[tokio::test]
async fn validate_recipient_certs_valid_wrong_anchor_and_index() {
    let store = test_store();
    let policy = StoreCryptoPolicy::new(&store, "a1");
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Gate CA");
    let other_ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Other CA");
    let leaf_a = astra_crypto_smime::testing::build_leaf_signed_by("ra@x.com", &ca);
    let leaf_b = astra_crypto_smime::testing::build_leaf_signed_by("rb@x.com", &ca);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Chains to the anchor: Ok (single and multiple).
    policy
        .validate_recipient_certs(
            std::slice::from_ref(&leaf_a.cert_der),
            std::slice::from_ref(&ca.cert_der),
            now,
        )
        .await
        .expect("valid chain");
    policy
        .validate_recipient_certs(
            &[leaf_a.cert_der.clone(), leaf_b.cert_der.clone()],
            std::slice::from_ref(&ca.cert_der),
            now,
        )
        .await
        .expect("multiple valid");

    // Wrong anchor: Err.
    let err = policy
        .validate_recipient_certs(
            std::slice::from_ref(&leaf_a.cert_der),
            std::slice::from_ref(&other_ca.cert_der),
            now,
        )
        .await
        .expect_err("a cert that does not chain is refused");
    assert!(err.contains("recipient cert #0 invalid"), "{err}");

    // The SECOND failing cert reports its own index.
    let err = policy
        .validate_recipient_certs(
            &[leaf_a.cert_der.clone(), leaf_b.cert_der.clone()],
            &[ca.cert_der.clone(), other_ca.cert_der.clone()],
            now + 300 * 24 * 60 * 60, // past the 200-day fixture validity
        )
        .await
        .expect_err("expired fixtures fail");
    assert!(
        err.contains("recipient cert #0 invalid"),
        "the first failure aborts: {err}"
    );
}
