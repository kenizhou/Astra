// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the cert-set queries: the anchor/intermediate split, the
//! harvested-cert trust semantics, and token-row lookup.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

fn insert_row(
    store: &CryptoStore,
    key_type: &str,
    origin: &str,
    der: &[u8],
    email: Option<&str>,
) -> String {
    let fp = astra_crypto_smime::fingerprint_of_cert_der(der).unwrap();
    crate::upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "smime".to_string(),
                key_type: key_type.to_string(),
                email: email.map(str::to_string),
                fingerprint: fp.clone(),
                origin: origin.to_string(),
                ..Default::default()
            },
            public_data: hex::encode(der),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
    fp
}

#[test]
fn imported_ca_certs_are_anchors_and_intermediates_never_are() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Anchor CA");
    let mid = astra_crypto_smime::testing::build_intermediate_signed_by("CN=Mid CA", &ca);
    insert_row(&store, "cert", "imported", &ca.cert_der, None);
    crate::upsert_intermediate_cert(&store, "a1", &mid.cert_der).unwrap();

    let anchors = list_trust_anchor_certs(&store, "a1").unwrap();
    assert_eq!(anchors, vec![ca.cert_der.clone()], "only the CA-root cert");

    let intermediates = list_intermediate_certs(&store, "a1").unwrap();
    assert_eq!(
        intermediates,
        vec![mid.cert_der.clone()],
        "the intermediate is chain evidence, kept out of the anchor set"
    );
}

#[test]
fn upsert_intermediate_cert_dedups_a_reimport() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Dup CA");
    crate::upsert_intermediate_cert(&store, "a1", &ca.cert_der).unwrap();
    crate::upsert_intermediate_cert(&store, "a1", &ca.cert_der).unwrap();
    assert_eq!(
        list_intermediate_certs(&store, "a1").unwrap().len(),
        1,
        "the UNIQUE constraint collapses a re-import"
    );
}

#[test]
fn harvested_certs_are_not_anchors_until_explicitly_accepted() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Harvest CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("peer@x.com", &ca);
    let fp = insert_row(
        &store,
        "cert",
        "harvested",
        &leaf.cert_der,
        Some("peer@x.com"),
    );

    assert!(
        list_trust_anchor_certs(&store, "a1").unwrap().is_empty(),
        "harvested certs never anchor by default"
    );

    // An explicit verified decision promotes the discovered cert into the
    // anchor set (the same acceptance that lets the send-side chain gate
    // pass for an otherwise-unchainable cert).
    crate::put_trust_decision(&store, "a1", "peer@x.com", "smime", &fp, "verified", None).unwrap();
    let anchors = list_trust_anchor_certs(&store, "a1").unwrap();
    assert_eq!(anchors.len(), 1, "explicit acceptance lifts the exclusion");

    // A NEWER decision that is not verified/personal re-applies the
    // exclusion (latest-wins).
    crate::put_trust_decision(&store, "a1", "peer@x.com", "smime", &fp, "rejected", None).unwrap();
    assert!(
        list_trust_anchor_certs(&store, "a1").unwrap().is_empty(),
        "a newer non-accepting decision re-excludes the cert"
    );
}

#[test]
fn insert_harvested_cert_is_a_no_op_when_the_fingerprint_is_known() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Known CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("known@x.com", &ca);

    assert!(
        insert_harvested_cert(&store, "a1", "known@x.com", &leaf.cert_der, None).unwrap(),
        "first harvest inserts"
    );
    // Re-harvest with a DIFFERENT email binding must not touch the row.
    assert!(
        !insert_harvested_cert(
            &store,
            "a1",
            "other@x.com",
            &leaf.cert_der,
            Some(r#"{"x":1}"#)
        )
        .unwrap(),
        "re-harvest is a strict no-op"
    );
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap();
    let row = crate::get_crypto_key_public(&store, "a1", "smime", &fp)
        .unwrap()
        .unwrap();
    assert_eq!(row.email.as_deref(), Some("known@x.com"), "no downgrade");
    assert_eq!(row.origin, "harvested");
    assert!(
        !row.has_private,
        "harvested rows never carry private material"
    );
}

#[test]
fn get_by_token_resolves_the_registration_row_without_decryption() {
    let store = test_store();
    crate::upsert_crypto_key(
        &store,
        &CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "smime".to_string(),
                key_type: "cert".to_string(),
                email: Some("token@x.com".to_string()),
                fingerprint: "fpTok".to_string(),
                origin: "imported".to_string(),
                token_serial: Some("SER123".to_string()),
                token_key_id: Some("key-1".to_string()),
                ..Default::default()
            },
            public_data: hex::encode(b"token-public-der"),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();

    let record = get_crypto_key_by_token(&store, "a1", "SER123", "key-1")
        .unwrap()
        .expect("token row resolves");
    assert_eq!(record.row.fingerprint, "fpTok");
    assert!(
        record.row.has_private,
        "a token row means the private op is available on-device"
    );
    assert!(
        record.private_data.is_none(),
        "token rows carry no soft blob"
    );
    assert_eq!(record.public_data, hex::encode(b"token-public-der"));

    // Wrong serial / wrong account are clean misses.
    assert!(get_crypto_key_by_token(&store, "a1", "SER999", "key-1")
        .unwrap()
        .is_none());
    assert!(get_crypto_key_by_token(&store, "a2", "SER123", "key-1")
        .unwrap()
        .is_none());
}
