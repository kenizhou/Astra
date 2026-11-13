// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the encrypted draft mirror: the put/get/delete round trip through
//! the vault, the AAD binding account and draft key, the absent-row answer,
//! and the typed keyring refusal.

use super::*;
use crate::store::CryptoStore;
use crate::vault::CryptoVaultError;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
    store
}

#[test]
fn put_get_delete_round_trips_through_the_vault() {
    let store = test_store();
    let words = b"the draft's own words, kept sealed at rest";
    put_encrypted_draft(&store, "a1", "draft-1@x", words).expect("put the mirror");

    let read = get_encrypted_draft(&store, "a1", "draft-1@x")
        .expect("get the mirror")
        .expect("the row is present");
    assert_eq!(read, words, "the vault answers the words that were put");

    delete_encrypted_draft(&store, "a1", "draft-1@x").expect("delete the mirror");
    assert_eq!(
        get_encrypted_draft(&store, "a1", "draft-1@x").expect("get after delete"),
        None,
        "a deleted mirror reads as absent"
    );
}

#[test]
fn the_stored_row_is_ciphertext_not_the_plaintext() {
    let store = test_store();
    let words = b"never on disk in the clear";
    put_encrypted_draft(&store, "a1", "draft-2@x", words).expect("put the mirror");

    let raw: String = store
        .conn()
        .query_row(
            "SELECT body_enc FROM encrypted_drafts WHERE account_id = 'a1' AND draft_key = 'draft-2@x'",
            [],
            |r| r.get(0),
        )
        .expect("the row is present");
    assert!(raw.starts_with("01"), "the v1 vault blob version byte");
    assert!(
        !raw.contains(&hex::encode(words)),
        "the plaintext must not ride the stored blob"
    );
}

#[test]
fn a_re_save_upserts_the_same_key() {
    let store = test_store();
    put_encrypted_draft(&store, "a1", "draft-3@x", b"first words").expect("first put");
    put_encrypted_draft(&store, "a1", "draft-3@x", b"second words").expect("re-save put");

    let read = get_encrypted_draft(&store, "a1", "draft-3@x")
        .expect("get the mirror")
        .expect("the row is present");
    assert_eq!(read, b"second words", "the re-save replaces the words");

    let count: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM encrypted_drafts WHERE account_id = 'a1' AND draft_key = 'draft-3@x'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "a re-save upserts, never stores a second row");
}

#[test]
fn a_missing_row_is_none() {
    let store = test_store();
    assert_eq!(
        get_encrypted_draft(&store, "a1", "never-written@x").expect("get a missing row"),
        None,
        "an absent mirror is Ok(None), never an error"
    );
}

#[test]
fn the_aad_binds_the_account_so_a_cross_account_blob_fails() {
    let store = test_store();
    put_encrypted_draft(&store, "a1", "draft-4@x", b"account one's words").expect("put under a1");

    // Lift the blob into a2's row under the same draft key: the AAD binds the
    // account, so the read under a2 must fail AEAD verification.
    let blob: String = store
        .conn()
        .query_row(
            "SELECT body_enc FROM encrypted_drafts WHERE account_id = 'a1' AND draft_key = 'draft-4@x'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO encrypted_drafts (account_id, draft_key, body_enc, updated_at)
             VALUES ('a2', 'draft-4@x', ?1, strftime('%s','now'))",
            rusqlite::params![blob],
        )
        .unwrap();

    let err = get_encrypted_draft(&store, "a2", "draft-4@x");
    assert!(
        matches!(err, Err(DraftMirrorError::Vault(CryptoVaultError::Aead(_)))),
        "a cross-account blob must fail AEAD: {err:?}"
    );
}

#[test]
fn the_aad_binds_the_draft_key_so_a_cross_key_blob_fails() {
    let store = test_store();
    put_encrypted_draft(&store, "a1", "draft-5@x", b"one draft's words").expect("put under key 5");

    let blob: String = store
        .conn()
        .query_row(
            "SELECT body_enc FROM encrypted_drafts WHERE account_id = 'a1' AND draft_key = 'draft-5@x'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO encrypted_drafts (account_id, draft_key, body_enc, updated_at)
             VALUES ('a1', 'draft-6@x', ?1, strftime('%s','now'))",
            rusqlite::params![blob],
        )
        .unwrap();

    let err = get_encrypted_draft(&store, "a1", "draft-6@x");
    assert!(
        matches!(err, Err(DraftMirrorError::Vault(CryptoVaultError::Aead(_)))),
        "a cross-key blob must fail AEAD: {err:?}"
    );
}

#[test]
fn delete_of_a_missing_row_is_ok() {
    let store = test_store();
    delete_encrypted_draft(&store, "a1", "never-written@x").expect("delete a missing row is ok");
}

#[test]
fn the_mirror_is_account_scoped() {
    let store = test_store();
    put_encrypted_draft(&store, "a1", "shared-key@x", b"a1 words").expect("put under a1");
    put_encrypted_draft(&store, "a2", "shared-key@x", b"a2 words").expect("put under a2");

    assert_eq!(
        get_encrypted_draft(&store, "a1", "shared-key@x")
            .unwrap()
            .unwrap(),
        b"a1 words"
    );
    assert_eq!(
        get_encrypted_draft(&store, "a2", "shared-key@x")
            .unwrap()
            .unwrap(),
        b"a2 words"
    );
    delete_encrypted_draft(&store, "a1", "shared-key@x").unwrap();
    assert_eq!(
        get_encrypted_draft(&store, "a2", "shared-key@x")
            .unwrap()
            .unwrap(),
        b"a2 words",
        "deleting one account's mirror leaves the other's intact"
    );
}
