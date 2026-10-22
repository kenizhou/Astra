// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the default signing key and deletion.

use super::*;
use crate::keys::{CryptoKeyRecord, CryptoKeyRow};
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

fn insert_key(store: &CryptoStore, standard: &str, fingerprint: &str) {
    crate::upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: standard.to_string(),
                key_type: "cert".to_string(),
                email: Some(format!("{fingerprint}@x.com")),
                fingerprint: fingerprint.to_string(),
                origin: "generated".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(b"blob"),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
}

#[test]
fn default_signing_key_get_set_and_switch() {
    let store = test_store();
    insert_key(&store, "openpgp", "fp1");
    insert_key(&store, "openpgp", "fp2");

    assert!(
        get_default_signing_key(&store, "a1", astra_crypto_core::Standard::OpenPgp)
            .unwrap()
            .is_none(),
        "no default flagged yet"
    );

    set_default_signing_key(&store, "a1", "openpgp", "fp1").unwrap();
    let row = get_default_signing_key(&store, "a1", astra_crypto_core::Standard::OpenPgp)
        .unwrap()
        .expect("default flagged");
    assert_eq!(row.fingerprint, "fp1");
    assert_eq!(row.email.as_deref(), Some("fp1@x.com"));

    // Switching the default un-flags the previous one atomically.
    set_default_signing_key(&store, "a1", "openpgp", "fp2").unwrap();
    let row = get_default_signing_key(&store, "a1", astra_crypto_core::Standard::OpenPgp)
        .unwrap()
        .unwrap();
    assert_eq!(row.fingerprint, "fp2", "the new default wins");
    let fp1 = crate::get_crypto_key_public(&store, "a1", "openpgp", "fp1")
        .unwrap()
        .unwrap();
    assert!(!fp1.is_default_sign, "the old default is un-flagged");

    // Standards are independent scopes.
    assert!(
        get_default_signing_key(&store, "a1", astra_crypto_core::Standard::Smime)
            .unwrap()
            .is_none()
    );
}

#[test]
fn setting_a_default_for_a_missing_key_errors_and_keeps_the_old_default() {
    let store = test_store();
    insert_key(&store, "openpgp", "fp1");
    set_default_signing_key(&store, "a1", "openpgp", "fp1").unwrap();

    let err = set_default_signing_key(&store, "a1", "openpgp", "fpGhost");
    assert!(
        err.is_err(),
        "a stale fingerprint is an error, not a silent clear"
    );

    // The un-flag UPDATE must have been rolled back: fp1 is still the default.
    let row = get_default_signing_key(&store, "a1", astra_crypto_core::Standard::OpenPgp)
        .unwrap()
        .expect("the prior default survives the failed switch");
    assert_eq!(row.fingerprint, "fp1");
}

#[test]
fn delete_is_idempotent_and_account_scoped() {
    let store = test_store();
    insert_key(&store, "openpgp", "fp1");

    delete_crypto_key(&store, "a1", "openpgp", "fp1").unwrap();
    assert!(crate::get_crypto_key_public(&store, "a1", "openpgp", "fp1")
        .unwrap()
        .is_none());
    // Second delete is a no-op, not an error.
    delete_crypto_key(&store, "a1", "openpgp", "fp1").unwrap();
    // Deleting under a different account never touches this account's row.
    insert_key(&store, "openpgp", "fp2");
    delete_crypto_key(&store, "a2", "openpgp", "fp2").unwrap();
    assert!(
        crate::get_crypto_key_public(&store, "a1", "openpgp", "fp2")
            .unwrap()
            .is_some(),
        "cross-account delete removes 0 rows"
    );
}

#[test]
fn backfill_cert_summaries_computes_display_metadata_for_null_rows() {
    let store = test_store();
    // A row written with NULL display metadata (the pre-backfill shape):
    // upsert computes them, so bypass it with a direct INSERT.
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Backfill CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("backfill@x.com", &ca);
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO crypto_keys (
                id, account_id, standard, key_type, email, fingerprint, public_data,
                private_data_enc, token_serial, token_key_id, origin,
                is_default_sign, is_default_encrypt, created_at, expires_at, policy_json,
                subject_cn, not_before
            ) VALUES (
                'bf1', 'a1', 'smime', 'cert', NULL, ?1, ?2,
                NULL, NULL, NULL, 'imported',
                0, 0, strftime('%s','now'), NULL, NULL,
                NULL, NULL
            )",
            rusqlite::params![fp, hex::encode(&leaf.cert_der)],
        )
        .unwrap();

    let updated = backfill_cert_summaries(&store).unwrap();
    assert_eq!(updated, 1);
    let row = crate::get_crypto_key_public(&store, "a1", "smime", &fp)
        .unwrap()
        .unwrap();
    assert!(row.not_before.is_some());
    assert_eq!(
        row.email.as_deref(),
        Some("backfill@x.com"),
        "email filled from the cert SAN"
    );

    // Idempotent: a second run touches nothing.
    assert_eq!(backfill_cert_summaries(&store).unwrap(), 0);
}
