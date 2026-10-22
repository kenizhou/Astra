// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the crypto_keys CRUD: round trips, conflict semantics, account
//! scoping, and the at-rest private-blob wrapping.

use super::*;
use crate::store::CryptoStore;

pub(crate) fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
    store
}

pub(crate) fn record(account: &str, standard: &str, fingerprint: &str) -> CryptoKeyRecord {
    CryptoKeyRecord {
        row: CryptoKeyRow {
            account_id: account.to_string(),
            standard: standard.to_string(),
            key_type: "cert".to_string(),
            email: Some(format!("{fingerprint}@x.com")),
            fingerprint: fingerprint.to_string(),
            origin: "generated".to_string(),
            ..Default::default()
        },
        public_data: hex::encode(b"fake-public-der"),
        private_data: None,
        policy_json: None,
    }
}

#[test]
fn upsert_then_get_public_round_trips_without_private_material() {
    let store = test_store();
    upsert_crypto_key(&store, &record("a1", "openpgp", "fpA")).unwrap();

    let row = get_crypto_key_public(&store, "a1", "openpgp", "fpA")
        .unwrap()
        .expect("row present");
    assert_eq!(row.fingerprint, "fpA");
    assert_eq!(row.standard, "openpgp");
    assert_eq!(row.origin, "generated");
    assert!(!row.has_private, "no private blob stored");
    assert_eq!(row.email.as_deref(), Some("fpA@x.com"));
    assert!(!row.id.is_empty(), "upsert generated a uuid");
    assert!(!row.created_at.is_empty(), "created_at stamped server-side");
}

#[test]
fn private_material_is_wrapped_at_rest_and_decrypted_by_get_full() {
    let store = test_store();
    let mut rec = record("a1", "openpgp", "fpP");
    rec.private_data = Some(hex::encode(b"secret-key-bytes"));
    upsert_crypto_key(&store, &rec).unwrap();

    // The public view flags the private half but never carries it.
    let row = get_crypto_key_public(&store, "a1", "openpgp", "fpP")
        .unwrap()
        .unwrap();
    assert!(row.has_private);

    // The raw column must NOT contain the plaintext hex: the vault wrapped it.
    let raw: String = store
        .conn()
        .query_row(
            "SELECT private_data_enc FROM crypto_keys WHERE account_id = 'a1' AND fingerprint = 'fpP'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(raw, hex::encode(b"secret-key-bytes"));
    assert!(raw.starts_with("01"), "v1 vault blob version byte");

    // get_full decrypts with the account-bound AAD.
    let full = get_crypto_key_full(&store, "a1", "openpgp", "fpP")
        .unwrap()
        .expect("record present");
    assert_eq!(
        full.private_data.as_deref(),
        Some(hex::encode(b"secret-key-bytes").as_str())
    );
}

#[test]
fn get_full_is_account_scoped_a_same_fingerprint_row_elsewhere_is_a_miss() {
    let store = test_store();
    let mut rec = record("a1", "openpgp", "fpShared");
    rec.private_data = Some(hex::encode(b"a1-secret"));
    upsert_crypto_key(&store, &rec).unwrap();

    // Account a2 must not resolve account a1's key, even by exact fingerprint.
    assert!(
        get_crypto_key_full(&store, "a2", "openpgp", "fpShared")
            .unwrap()
            .is_none(),
        "cross-account get must be a miss, never another account's key"
    );
    assert!(get_crypto_key_public(&store, "a2", "openpgp", "fpShared")
        .unwrap()
        .is_none());
}

#[test]
fn conflict_update_preserves_id_created_at_and_origin() {
    let store = test_store();
    upsert_crypto_key(&store, &record("a1", "smime", "fpC")).unwrap();
    let first = get_crypto_key_public(&store, "a1", "smime", "fpC")
        .unwrap()
        .unwrap();

    let mut again = record("a1", "smime", "fpC");
    again.row.origin = "imported".to_string(); // must NOT take effect
    again.row.key_type = "public".to_string();
    upsert_crypto_key(&store, &again).unwrap();
    let second = get_crypto_key_public(&store, "a1", "smime", "fpC")
        .unwrap()
        .unwrap();

    assert_eq!(first.id, second.id, "id preserved on conflict");
    assert_eq!(first.created_at, second.created_at, "created_at preserved");
    assert_eq!(second.origin, "generated", "origin preserved on conflict");
    assert_eq!(second.key_type, "public", "other fields update");
}

#[test]
fn conflict_update_with_no_private_data_does_not_wipe_the_stored_blob() {
    let store = test_store();
    let mut rec = record("a1", "openpgp", "fpK");
    rec.private_data = Some(hex::encode(b"keep-me"));
    upsert_crypto_key(&store, &rec).unwrap();

    // A re-upsert carrying no private material must not clear the stored blob
    // (the COALESCE clause).
    let again = record("a1", "openpgp", "fpK");
    upsert_crypto_key(&store, &again).unwrap();
    let full = get_crypto_key_full(&store, "a1", "openpgp", "fpK")
        .unwrap()
        .unwrap();
    assert_eq!(
        full.private_data.as_deref(),
        Some(hex::encode(b"keep-me").as_str()),
        "the stored private blob survives a private-less re-upsert"
    );
}

#[test]
fn list_for_email_and_account_are_deterministic() {
    let store = test_store();
    for fp in ["fpB", "fpA", "fpC"] {
        let mut rec = record("a1", "openpgp", fp);
        rec.row.email = Some("shared@x.com".to_string());
        upsert_crypto_key(&store, &rec).unwrap();
    }
    let by_email = list_crypto_keys_for_email(&store, "openpgp", "shared@x.com").unwrap();
    let fps: Vec<&str> = by_email.iter().map(|r| r.fingerprint.as_str()).collect();
    assert_eq!(fps, ["fpA", "fpB", "fpC"], "ordered by fingerprint");

    let by_account = list_crypto_keys_for_account(&store, "a1", "openpgp").unwrap();
    assert_eq!(by_account.len(), 3);
    assert!(
        list_crypto_keys_for_account(&store, "a2", "openpgp")
            .unwrap()
            .is_empty(),
        "account scoping"
    );
}

#[test]
fn get_public_with_blob_returns_the_hex_public_data() {
    let store = test_store();
    upsert_crypto_key(&store, &record("a1", "openpgp", "fpBlob")).unwrap();
    let (row, blob) = get_crypto_key_public_with_blob(&store, "a1", "openpgp", "fpBlob")
        .unwrap()
        .expect("present");
    assert_eq!(row.fingerprint, "fpBlob");
    assert_eq!(blob, hex::encode(b"fake-public-der"));
    assert!(
        get_crypto_key_public_with_blob(&store, "a1", "openpgp", "fpNope")
            .unwrap()
            .is_none()
    );
}

#[test]
fn unparseable_smime_blob_leaves_display_metadata_null() {
    let store = test_store();
    // "fake-public-der" is not a cert: subject_cn / not_before stay NULL and
    // the caller-supplied email is kept.
    upsert_crypto_key(&store, &record("a1", "smime", "fpX")).unwrap();
    let row = get_crypto_key_public(&store, "a1", "smime", "fpX")
        .unwrap()
        .unwrap();
    assert!(row.subject_cn.is_none());
    assert!(row.not_before.is_none());
    assert_eq!(row.email.as_deref(), Some("fpX@x.com"));
}

#[test]
fn a_real_smime_cert_fills_the_display_metadata() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Keys Test CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("leaf@x.com", &ca);
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap();
    let rec = CryptoKeyRecord {
        row: CryptoKeyRow {
            account_id: "a1".to_string(),
            standard: "smime".to_string(),
            key_type: "cert".to_string(),
            fingerprint: fp.clone(),
            origin: "imported".to_string(),
            ..Default::default()
        },
        public_data: hex::encode(&leaf.cert_der),
        private_data: None,
        policy_json: None,
    };
    upsert_crypto_key(&store, &rec).unwrap();
    let row = get_crypto_key_public(&store, "a1", "smime", &fp)
        .unwrap()
        .unwrap();
    assert!(
        row.not_before.is_some(),
        "not_before computed from the cert"
    );
    assert_eq!(
        row.email.as_deref(),
        Some("leaf@x.com"),
        "email filled from the cert SAN when unset"
    );
}
