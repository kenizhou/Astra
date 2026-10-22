// SPDX-License-Identifier: GPL-3.0-only
//! The default signing key and key deletion: kylins' `crypto_keys/default.rs`
//! ported.

use crate::store::CryptoStore;

/// Minimal view of a default signing key: the bits the send gate needs to
/// build a `KeyHandleRef` and resolve the cert+key via
/// [`crate::get_crypto_key_full`]. Carries no private material.
#[derive(Debug, Clone)]
pub struct DefaultKeyRow {
    /// `'openpgp' | 'smime' | 'sm'`.
    pub standard: String,
    /// The default key's fingerprint.
    pub fingerprint: String,
    /// The identity the key signs for.
    pub email: Option<String>,
    /// Present when the default signing key lives on a hardware token; the
    /// handle-ref builder then builds a `KeyHandle::Token` (not Software).
    pub token_serial: Option<String>,
    /// The key's id on the token, when token-backed.
    pub token_key_id: Option<String>,
}

/// The account's default signing key for `standard`, if one is flagged.
/// Returns just enough to build a `KeyHandleRef`; callers needing the
/// cert/private blob follow up with [`crate::get_crypto_key_full`] using the
/// returned fingerprint.
///
/// # Errors
/// Fails on a database error.
pub fn get_default_signing_key(
    store: &CryptoStore,
    account_id: &str,
    standard: astra_crypto_core::Standard,
) -> Result<Option<DefaultKeyRow>, String> {
    store
        .conn()
        .query_row(
            "SELECT standard, fingerprint, email, token_serial, token_key_id FROM crypto_keys
             WHERE account_id = ?1 AND standard = ?2 AND is_default_sign = 1
             LIMIT 1",
            rusqlite::params![account_id, standard.as_str()],
            |row| {
                Ok(DefaultKeyRow {
                    standard: row.get(0)?,
                    fingerprint: row.get(1)?,
                    email: row.get(2)?,
                    token_serial: row.get(3)?,
                    token_key_id: row.get(4)?,
                })
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// Delete a key by `(account_id, standard, fingerprint)`. Idempotent:
/// deleting a fingerprint that doesn't exist is a no-op (no error, no rows
/// affected). Private material is removed along with the rest of the row.
///
/// # Errors
/// Fails when the delete cannot run.
pub fn delete_crypto_key(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<(), String> {
    store
        .conn()
        .execute(
            "DELETE FROM crypto_keys WHERE account_id = ?1 AND standard = ?2 AND fingerprint = ?3",
            rusqlite::params![account_id, standard, fingerprint],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Atomically set the default signing key for `(account_id, standard)`:
/// un-flag every existing default in the scope, then flag the one identified
/// by `fingerprint`. Errors if no row matches, surfacing a stale UI rather
/// than silently leaving the account without a default. Both UPDATEs run in
/// a single transaction; the un-flag is rolled back via the dropped
/// (never-committed) transaction on the missing-fingerprint path.
///
/// # Errors
/// Fails when no row matches `(account_id, standard, fingerprint)`, or on a database error.
pub fn set_default_signing_key(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let mut guard = store.conn();
    let tx = guard.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE crypto_keys SET is_default_sign = 0 WHERE account_id = ?1 AND standard = ?2",
        rusqlite::params![account_id, standard],
    )
    .map_err(|e| e.to_string())?;
    let affected = tx
        .execute(
            "UPDATE crypto_keys SET is_default_sign = 1
             WHERE account_id = ?1 AND standard = ?2 AND fingerprint = ?3",
            rusqlite::params![account_id, standard, fingerprint],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        // Returning `Err` here drops `tx` without committing: the un-flag
        // UPDATE above is rolled back, so the prior default is left intact.
        return Err(format!("no key for {account_id}/{standard}/{fingerprint}"));
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// One-time backfill for rows written before the `subject_cn`/`not_before`
/// columns existed: their X.509 display metadata is computed from the stored
/// cert blob. Idempotent (only touches `subject_cn IS NULL` rows) and
/// best-effort per row (one unparseable blob doesn't stop the rest).
/// A failure here must never block app startup.
///
/// OpenPGP rows are excluded up front (their `public_data` is not X.509);
/// S/MIME rows with non-hex or non-cert blobs are skipped, so they are
/// re-scanned on the next startup.
///
/// # Errors
/// Fails on a database error; an unparseable cert blob is skipped, never an error.
pub fn backfill_cert_summaries(store: &CryptoStore) -> Result<usize, String> {
    let rows: Vec<(String, String)> = {
        let conn = store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, public_data FROM crypto_keys
                 WHERE subject_cn IS NULL AND standard IN ('smime','sm')",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    let mut updated = 0usize;
    for (id, public_data) in rows {
        let Ok(der) = hex::decode(&public_data) else {
            continue;
        };
        let Ok(summary) = astra_crypto_smime::cert_summary_of_der(&der) else {
            continue;
        };
        store
            .conn()
            .execute(
                "UPDATE crypto_keys SET
                    subject_cn = ?1,
                    not_before = ?2,
                    email = COALESCE(email, ?3)
                 WHERE id = ?4",
                rusqlite::params![
                    summary.subject_cn,
                    summary.not_before_unix.to_string(),
                    summary.email,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
        updated += 1;
    }
    if updated > 0 {
        log::info!("[crypto] backfilled cert display metadata for {updated} key row(s)");
    }
    Ok(updated)
}

#[cfg(test)]
#[path = "default_tests.rs"]
mod default_tests;
