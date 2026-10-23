// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the collected-keys staging layer: staging, dedup, listing,
//! clearing, and the explicit promote path with its no-downgrade guard.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

#[test]
fn stage_then_list_for_peer_newest_first() {
    let store = test_store();
    let id = stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("openpgp"),
        Some("fp1"),
        b"blob-1",
        Some("wkd"),
    )
    .unwrap();
    assert!(id > 0);
    stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("openpgp"),
        Some("fp2"),
        b"blob-2",
        Some("autocrypt"),
    )
    .unwrap();

    let rows = list_collected_keys_for_peer(&store, "a1", "peer@x.com", "openpgp").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].fingerprint.as_deref(),
        Some("fp2"),
        "newest first (id DESC tie-break)"
    );
    assert_eq!(rows[0].public_data, b"blob-2");
    assert_eq!(rows[0].source.as_deref(), Some("autocrypt"));
    assert!(!rows[0].seen_at.is_empty(), "seen_at stamped server-side");

    // Filters scope correctly: another peer / standard sees nothing.
    assert!(
        list_collected_keys_for_peer(&store, "a1", "other@x.com", "openpgp")
            .unwrap()
            .is_empty()
    );
    assert!(
        list_collected_keys_for_peer(&store, "a1", "peer@x.com", "smime")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn stage_if_absent_dedups_the_same_key_from_the_same_source() {
    let store = test_store();
    let first = stage_collected_key_if_absent(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("smime"),
        Some("fp1"),
        b"blob",
        Some("smime-harvest-intermediate"),
    )
    .unwrap();
    assert!(first.is_some(), "first stage inserts");
    let second = stage_collected_key_if_absent(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("smime"),
        Some("fp1"),
        b"blob",
        Some("smime-harvest-intermediate"),
    )
    .unwrap();
    assert!(second.is_none(), "a repeat is a no-op, not a duplicate row");

    // A DIFFERENT source for the same key stages its own row.
    let third = stage_collected_key_if_absent(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("smime"),
        Some("fp1"),
        b"blob",
        Some("ldap"),
    )
    .unwrap();
    assert!(third.is_some());
}

#[test]
fn remove_and_clear_scoped_to_the_account() {
    let store = test_store();
    let id = stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("openpgp"),
        Some("fp1"),
        b"b",
        Some("wkd"),
    )
    .unwrap();
    stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("openpgp"),
        Some("fp2"),
        b"b",
        Some("wkd"),
    )
    .unwrap();

    remove_collected_key(&store, id).unwrap();
    assert_eq!(
        list_collected_keys_for_account(&store, "a1", "openpgp")
            .unwrap()
            .len(),
        1
    );

    let cleared = clear_collected_keys(&store, "a1").unwrap();
    assert_eq!(cleared, 1);
    assert!(list_collected_keys_for_account(&store, "a1", "openpgp")
        .unwrap()
        .is_empty());
}

#[test]
fn promote_moves_material_into_crypto_keys_and_clears_staging() {
    let store = test_store();
    stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("openpgp"),
        Some("fpP"),
        b"pgp-public-blob",
        Some("wkd"),
    )
    .unwrap();

    let row = promote_collected_key(&store, "a1", "openpgp", "fpP", Some("peer@x.com")).unwrap();
    assert_eq!(row.fingerprint, "fpP");
    assert_eq!(row.key_type, "public", "OpenPGP promotes as 'public'");
    assert_eq!(row.origin, "wkd", "provenance preserved");
    assert_eq!(row.email.as_deref(), Some("peer@x.com"));
    assert!(
        list_collected_keys_for_fingerprint(&store, "a1", "openpgp", "fpP")
            .unwrap()
            .is_empty(),
        "staging drains after a successful promote"
    );

    // The keystore row carries the hex-encoded staged blob.
    let (_, blob) = crate::get_crypto_key_public_with_blob(&store, "a1", "openpgp", "fpP")
        .unwrap()
        .unwrap();
    assert_eq!(blob, hex::encode(b"pgp-public-blob"));
}

#[test]
fn promote_never_rewrites_an_existing_keystore_row() {
    let store = test_store();
    // An imported keystore row already exists for the fingerprint.
    crate::upsert_crypto_key(
        &store,
        &crate::CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "openpgp".to_string(),
                key_type: "cert".to_string(),
                email: Some("imported@x.com".to_string()),
                fingerprint: "fpD".to_string(),
                origin: "imported".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(b"imported-blob"),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
    // A staged duplicate with a different email/source.
    stage_collected_key(
        &store,
        Some("a1"),
        Some("staged@x.com"),
        Some("openpgp"),
        Some("fpD"),
        b"staged-blob",
        Some("keyserver"),
    )
    .unwrap();

    let row = promote_collected_key(&store, "a1", "openpgp", "fpD", Some("staged@x.com")).unwrap();
    assert_eq!(row.origin, "imported", "no origin downgrade");
    assert_eq!(
        row.email.as_deref(),
        Some("imported@x.com"),
        "the existing row is left completely untouched"
    );
    let (_, blob) = crate::get_crypto_key_public_with_blob(&store, "a1", "openpgp", "fpD")
        .unwrap()
        .unwrap();
    assert_eq!(blob, hex::encode(b"imported-blob"));
    assert!(
        list_collected_keys_for_fingerprint(&store, "a1", "openpgp", "fpD")
            .unwrap()
            .is_empty(),
        "staging still drains (the key is in the keystore)"
    );
}

#[test]
fn promote_errors_when_the_material_is_missing_entirely() {
    let store = test_store();
    let err = promote_collected_key(&store, "a1", "openpgp", "fpGhost", None);
    assert!(
        err.is_err(),
        "no staged row AND no keystore row is an error"
    );
}

#[test]
fn promote_maps_unknown_sources_to_imported() {
    let store = test_store();
    stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("openpgp"),
        Some("fpU"),
        b"blob",
        Some("attachment"),
    )
    .unwrap();
    let row = promote_collected_key(&store, "a1", "openpgp", "fpU", None).unwrap();
    assert_eq!(row.origin, "imported", "unknown sources land as 'imported'");
}

#[test]
fn promote_smime_ca_cert_lands_as_intermediate_never_anchor() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Staged CA");
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&ca.cert_der).unwrap();
    stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("smime"),
        Some(&fp),
        &ca.cert_der,
        Some("smime-harvest-intermediate"),
    )
    .unwrap();

    let row = promote_collected_key(&store, "a1", "smime", &fp, Some("peer@x.com")).unwrap();
    assert_eq!(
        row.key_type, "intermediate",
        "a staged CA cert promotes as chain evidence, never an anchor"
    );
    assert_eq!(row.origin, "harvested");
    assert!(
        crate::list_trust_anchor_certs(&store, "a1")
            .unwrap()
            .is_empty(),
        "intermediates stay out of the anchor set"
    );
    assert_eq!(
        crate::list_intermediate_certs(&store, "a1").unwrap().len(),
        1
    );
}

#[test]
fn promote_smime_leaf_lands_as_cert() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Leaf CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("leaf@x.com", &ca);
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap();
    stage_collected_key(
        &store,
        Some("a1"),
        Some("leaf@x.com"),
        Some("smime"),
        Some(&fp),
        &leaf.cert_der,
        Some("harvested"),
    )
    .unwrap();

    let row = promote_collected_key(&store, "a1", "smime", &fp, None).unwrap();
    assert_eq!(row.key_type, "cert", "end-entity leaf certs keep 'cert'");
    assert_eq!(row.origin, "harvested");
    assert_eq!(
        row.email.as_deref(),
        Some("leaf@x.com"),
        "the staged peer email binds the row"
    );
}
