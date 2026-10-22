// SPDX-License-Identifier: GPL-3.0-only
//! S/MIME cert-set queries over `crypto_keys`: trust anchors, intermediates,
//! harvested recipient certs, and token-key lookup. Ported from kylins'
//! `crypto_keys/certs.rs`; the trust semantics comments are load-bearing and
//! carried verbatim in spirit.

use crate::keys::{cert_summary_fields, full_row_to_record, CryptoKeyRecord, FULL_COLS};
use crate::store::CryptoStore;

/// Return the raw DER bytes of every S/MIME CA-root trust anchor the user
/// has imported for this account (`crypto_keys` rows with `standard='smime'`
/// AND `key_type='cert'`). These feed the chain validator's
/// `trust_anchor_ders` argument.
///
/// Personal signing keys and public-only certs are excluded: only CA-root
/// entries in the "Trusted CAs" store qualify. Returns an empty Vec (not an
/// error) when the account has no imported trust anchors.
///
/// # Harvested/LDAP-origin exclusion
///
/// Rows with `origin IN ('harvested','ldap')` are NOT anchors by default: a
/// cert harvested from an incoming SignedData or staged from an LDAP
/// directory is unvetted, so letting it anchor path validation would flip
/// that signer's later messages from `Invalid` to chain-valid with zero user
/// involvement, a trust overreach. The exclusion lifts ONLY when the user
/// has made an explicit trust decision (`decision IN ('verified','personal')`)
/// that is the LATEST decision for its `(peer_email, fingerprint)` bucket
/// (same latest-wins rule as [`crate::get_latest_trust_decision`]): explicit
/// acceptance is what promotes a discovered cert into the anchor set, which
/// is also what makes the send-side recipient-cert gate pass for an
/// accepted, otherwise-unchainable discovered cert.
///
/// `public_data` is stored HEX-encoded; rows that fail to decode are skipped
/// with a `log::warn!` (one bad anchor must not brick the rest).
///
/// # Errors
/// Fails on a database error.
pub fn list_trust_anchor_certs(
    store: &CryptoStore,
    account_id: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let conn = store.conn();
    let mut stmt = conn
        .prepare(
            "SELECT ck.public_data FROM crypto_keys ck
             WHERE ck.account_id = ?1 AND ck.standard = 'smime' AND ck.key_type = 'cert'
               AND (
                     ck.origin NOT IN ('harvested','ldap')
                     OR EXISTS (
                         SELECT 1 FROM trust_decisions td
                         WHERE td.account_id = ck.account_id
                           AND td.standard = ck.standard
                           AND td.fingerprint = ck.fingerprint
                           AND td.decision IN ('verified','personal')
                           AND NOT EXISTS (
                               SELECT 1 FROM trust_decisions newer
                               WHERE newer.account_id = td.account_id
                                 AND newer.peer_email = td.peer_email
                                 AND newer.standard = td.standard
                                 AND newer.fingerprint = td.fingerprint
                                 AND (newer.decided_at > td.decided_at
                                      OR (newer.decided_at = td.decided_at AND newer.id > td.id))
                           )
                     )
                   )",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![account_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for hex_str in rows {
        let hex_str = hex_str.map_err(|e| e.to_string())?;
        match hex::decode(&hex_str) {
            Ok(der) => out.push(der),
            Err(e) => log::warn!("[crypto] trust-anchor public_data hex decode failed: {e}"),
        }
    }
    Ok(out)
}

/// Return the raw DER bytes of every stored S/MIME **intermediate** CA cert
/// for this account (`key_type='intermediate'`). Intermediates are
/// deliberately kept OUT of the trust-anchor candidate set; an intermediate
/// as an anchor would let any cert it signs validate as trusted, a trust
/// overreach. Rows that fail to hex-decode are skipped with a `log::warn!`.
///
/// # Errors
/// Fails on a database error.
pub fn list_intermediate_certs(
    store: &CryptoStore,
    account_id: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let conn = store.conn();
    let mut stmt = conn
        .prepare(
            "SELECT public_data FROM crypto_keys
             WHERE account_id = ?1 AND standard = 'smime' AND key_type = 'intermediate'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![account_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for hex_str in rows {
        let hex_str = hex_str.map_err(|e| e.to_string())?;
        match hex::decode(&hex_str) {
            Ok(der) => out.push(der),
            Err(e) => log::warn!("[crypto] intermediate public_data hex decode failed: {e}"),
        }
    }
    Ok(out)
}

/// Persist an S/MIME intermediate CA cert (DER) for `account_id` with
/// `key_type='intermediate'`, a direct INSERT that bypasses the keystore
/// bridge's `put` (which hardcodes `key_type='cert'`, which would land the
/// intermediate in [`list_trust_anchor_certs`]'s candidate-anchor set, a
/// trust overreach). The fingerprint is SHA-1-of-SPKI hex via
/// [`astra_crypto_smime::fingerprint_of_cert_der`], identical to the leaf
/// computation, so the UNIQUE constraint dedups a re-import of the same
/// `.p12` to a single row. Idempotent.
///
/// # Errors
/// Fails when the fingerprint cannot be computed or the upsert cannot run.
pub fn upsert_intermediate_cert(
    store: &CryptoStore,
    account_id: &str,
    cert_der: &[u8],
) -> Result<(), String> {
    let fingerprint = astra_crypto_smime::fingerprint_of_cert_der(cert_der)
        .map_err(|e| format!("upsert_intermediate_cert: compute fingerprint: {e}"))?;
    let (subject_cn, not_before, _email) = cert_summary_fields(cert_der);

    let id = uuid::Uuid::new_v4().to_string();
    let public_hex = hex::encode(cert_der);

    store
        .conn()
        .execute(
            "INSERT INTO crypto_keys (
                id, account_id, standard, key_type, email, fingerprint, public_data,
                private_data_enc, token_serial, token_key_id, origin,
                is_default_sign, is_default_encrypt, created_at, expires_at, policy_json,
                subject_cn, not_before
            ) VALUES (
                ?1, ?2, 'smime', 'intermediate', NULL, ?3, ?4,
                NULL, NULL, NULL, 'p12-intermediate',
                0, 0, strftime('%s','now'), NULL, NULL,
                ?5, ?6
            )
            ON CONFLICT(account_id, standard, fingerprint) DO UPDATE SET
                key_type = excluded.key_type,
                public_data = excluded.public_data,
                origin = excluded.origin,
                is_default_sign = excluded.is_default_sign,
                is_default_encrypt = excluded.is_default_encrypt,
                subject_cn = excluded.subject_cn,
                not_before = excluded.not_before",
            rusqlite::params![
                id,
                account_id,
                fingerprint,
                public_hex,
                subject_cn,
                not_before
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Persist an S/MIME cert discovered by harvesting an incoming SignedData
/// (the signer leaf) with `key_type='cert'` + `origin='harvested'`. Public
/// cert only: `private_data_enc` is always NULL, `is_default_*` always 0, so
/// a harvested row can never become a signing key or a send-side default.
///
/// # No-op-when-known (the origin-downgrade guard)
///
/// When a row already exists for the fingerprint (ANY origin), this is a
/// strict no-op returning `Ok(false)`: the existing row is left completely
/// untouched, so an `imported`/`generated` row is never downgraded to
/// `'harvested'`. This is deliberately NOT routed through
/// [`crate::upsert_crypto_key`], whose ON CONFLICT clause would still
/// overwrite `email` / `public_data` / `policy_json` on the existing row.
///
/// # Trust semantics
///
/// No `trust_decisions` row is written: the cert enters as Undecided.
/// [`list_trust_anchor_certs`] excludes `'harvested'` rows from the anchor
/// set until the user makes an explicit verified/personal decision, so
/// harvesting alone never flips a later signature to chain-valid.
///
/// Returns `Ok(true)` when a row was inserted, `Ok(false)` when the cert was
/// already known.
///
/// # Errors
/// Fails when the fingerprint cannot be computed or the insert cannot run.
pub fn insert_harvested_cert(
    store: &CryptoStore,
    account_id: &str,
    email: &str,
    cert_der: &[u8],
    policy_json: Option<&str>,
) -> Result<bool, String> {
    let fingerprint = astra_crypto_smime::fingerprint_of_cert_der(cert_der)
        .map_err(|e| format!("insert_harvested_cert: compute fingerprint: {e}"))?;

    // No-op-when-known: any existing row for the fingerprint (any origin) is
    // left untouched; never downgrade an imported/generated row to
    // 'harvested'.
    let exists: Option<i64> = store
        .conn()
        .query_row(
            "SELECT 1 FROM crypto_keys
             WHERE account_id = ?1 AND standard = 'smime' AND fingerprint = ?2 LIMIT 1",
            rusqlite::params![account_id, fingerprint],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })?;
    if exists.is_some() {
        return Ok(false);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let public_hex = hex::encode(cert_der);
    let (subject_cn, not_before, _email) = cert_summary_fields(cert_der);
    store
        .conn()
        .execute(
            "INSERT INTO crypto_keys (
                id, account_id, standard, key_type, email, fingerprint, public_data,
                private_data_enc, token_serial, token_key_id, origin,
                is_default_sign, is_default_encrypt, created_at, expires_at, policy_json,
                subject_cn, not_before
            ) VALUES (
                ?1, ?2, 'smime', 'cert', ?3, ?4, ?5,
                NULL, NULL, NULL, 'harvested',
                0, 0, strftime('%s','now'), NULL, ?6,
                ?7, ?8
            )",
            rusqlite::params![
                id,
                account_id,
                email,
                fingerprint,
                public_hex,
                policy_json,
                subject_cn,
                not_before
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Return the full record for a TOKEN key identified by `(account_id,
/// token_serial, token_key_id)`, or `None`. **In-Rust only.** Token rows
/// carry no private blob (`private_data_enc` is NULL by construction), so
/// this performs no decryption; it exists so the [`crate::SqliteKeyStore`]
/// can resolve `KeyHandle::Token` handles to the registration-time public
/// material. Account-scoped like [`crate::get_crypto_key_full`].
///
/// # Errors
/// Fails on a database error.
pub fn get_crypto_key_by_token(
    store: &CryptoStore,
    account_id: &str,
    token_serial: &str,
    token_key_id: &str,
) -> Result<Option<CryptoKeyRecord>, String> {
    let sql = format!(
        "SELECT {FULL_COLS} FROM crypto_keys WHERE account_id = ?1 AND token_serial = ?2 AND token_key_id = ?3"
    );
    store
        .conn()
        .query_row(
            &sql,
            rusqlite::params![account_id, token_serial, token_key_id],
            |row| {
                let mut record = full_row_to_record(row, false)?;
                // Token row: the private op is available on-device.
                record.row.has_private = true;
                Ok(record)
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

#[cfg(test)]
#[path = "certs_tests.rs"]
mod certs_tests;
