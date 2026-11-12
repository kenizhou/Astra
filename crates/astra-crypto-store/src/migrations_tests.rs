// SPDX-License-Identifier: GPL-3.0-only
//! The SP4c-2 schema migration proofs: the `standard` discriminator column,
//! the `'stale'` revocation_state CHECK extension (a table rebuild), and the
//! backward-compatible preservation of existing rows and verdict spellings.

use crate::store::CryptoStore;

fn store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    CryptoStore::open_in_memory().expect("open in-memory store")
}

/// Insert a result row with the given revocation_state + standard, returning
/// whether the write succeeded.
fn insert_row(store: &CryptoStore, msg: &str, revocation: &str, standard: Option<&str>) -> bool {
    store
        .conn()
        .execute(
            "INSERT INTO message_crypto_results
                (account_id, message_id, crypto_kind, decrypt_state, signature_state,
                 revocation_state, standard, verified_at)
             VALUES ('a1', ?1, 'encrypted', 'ok', 'valid-verified', ?2, ?3, '1')",
            rusqlite::params![msg, revocation, standard],
        )
        .is_ok()
}

#[test]
fn the_standard_column_accepts_a_discriminator() {
    let store = store();
    assert!(
        insert_row(&store, "m1", "unchecked", Some("smime")),
        "the standard column must accept 'smime'"
    );
    let standard: Option<String> = store
        .conn()
        .query_row(
            "SELECT standard FROM message_crypto_results WHERE message_id = 'm1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(standard.as_deref(), Some("smime"));
}

#[test]
fn the_revocation_state_check_admits_stale() {
    let store = store();
    assert!(
        insert_row(&store, "m2", "stale", None),
        "'stale' must be a valid revocation_state after the rebuild"
    );
}

#[test]
fn the_existing_revocation_states_still_validate() {
    let store = store();
    for (msg, state) in [("m3", "good"), ("m4", "revoked"), ("m5", "unchecked")] {
        assert!(
            insert_row(&store, msg, state, None),
            "{state} must stay valid"
        );
    }
    // An unknown spelling is still refused (the CHECK is extended, not dropped).
    assert!(
        !insert_row(&store, "m6", "bogus", None),
        "an unknown revocation_state must still be refused"
    );
}

#[test]
fn the_standard_column_defaults_to_null_for_legacy_rows() {
    let store = store();
    // A row written without a standard (the pre-migration shape) keeps NULL.
    assert!(insert_row(&store, "m7", "unchecked", None));
    let standard: Option<String> = store
        .conn()
        .query_row(
            "SELECT standard FROM message_crypto_results WHERE message_id = 'm7'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(standard, None);
}

#[test]
#[allow(clippy::type_complexity)]
fn the_rebuild_preserves_existing_rows_verbatim() {
    // A file-backed store at v3 holding a legacy verdict row, reopened so the
    // 0004 table rebuild runs: the row must survive verbatim, the new columns
    // NULL, the PK intact (the data-loss surface of a security-verdict rebuild).
    let path =
        std::env::temp_dir().join(format!("astra-mig-preserve-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let conn = rusqlite::Connection::open(&path).expect("open the raw v3 store");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                 version INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 applied_at TEXT NOT NULL DEFAULT (strftime('%s','now'))
             )",
        )
        .unwrap();
        for migration in &super::MIGRATIONS[..3] {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO _migrations (version, name) VALUES (?1, ?2)",
                rusqlite::params![migration.version, migration.name],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO message_crypto_results
                (account_id, message_id, crypto_kind, decrypt_state, signature_state,
                 signer_fingerprint, chain_valid, revocation_state, verified_at)
             VALUES ('a1', 'legacy', 'encrypted', 'ok', 'valid-verified', 'fp', 1, 'good', '1')",
            [],
        )
        .unwrap();
    }
    let store = CryptoStore::open(&path).expect("the reopen applies the 0004 rebuild");
    let row: (
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = store
        .conn()
        .query_row(
            "SELECT crypto_kind, decrypt_state, signature_state, signer_fingerprint,
                    chain_valid, revocation_state, standard, failure_reason, revocation_reason
             FROM message_crypto_results WHERE account_id = 'a1' AND message_id = 'legacy'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .expect("the legacy row survives the rebuild");
    assert_eq!(row.0, "encrypted");
    assert_eq!(row.1, "ok");
    assert_eq!(row.2, "valid-verified");
    assert_eq!(row.3.as_deref(), Some("fp"));
    assert_eq!(row.4, Some(1));
    assert_eq!(row.5, "good");
    assert!(
        row.6.is_none() && row.7.is_none() && row.8.is_none(),
        "the new columns stay NULL for the legacy row"
    );
    let _ = std::fs::remove_file(&path);
}
