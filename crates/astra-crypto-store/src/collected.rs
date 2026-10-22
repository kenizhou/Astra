// SPDX-License-Identifier: GPL-3.0-only
//! Collected (staging) keys query layer: kylins' `collected_keys.rs` ported.
//!
//! Owns the `collected_keys` table, the **silent staging** area for keys
//! observed via discovery (WKD/keyserver/autocrypt/contact) but not yet
//! accepted into `crypto_keys`. Staged rows are listed for UI review and
//! removed once the user accepts or rejects them. Promotion into
//! `crypto_keys` is the EXPLICIT acceptance flow; it is never silent.

use serde::{Deserialize, Serialize};

use crate::keys::CryptoKeyRow;
use crate::store::CryptoStore;

/// One collected-key staging row. Mirrors the `collected_keys` table; JSON
/// keys are camelCase for the frontend. `public_data` is the raw key bytes
/// (armored PGP / DER) as observed at discovery time.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CollectedKeyRow {
    /// Autoincrement row id.
    pub id: i64,
    /// The account that observed the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// The peer address the key was observed for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_email: Option<String>,
    /// `'openpgp' | 'smime' | 'sm'`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard: Option<String>,
    /// The staged key's fingerprint, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// The raw key bytes (armored PGP / DER) as observed at discovery time.
    #[serde(default, with = "serde_bytes_option")]
    pub public_data: Vec<u8>,
    /// The discovery source (`'wkd'`, `'autocrypt'`, `'harvested'`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Unix-seconds string, stamped server-side at insert.
    pub seen_at: String,
}

/// serde helper: serialize `Vec<u8>` as a base64 string (or absent when
/// empty), so the frontend receives a portable representation rather than a
/// raw byte array. Empty vec serializes as `None`.
mod serde_bytes_option {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        if v.is_empty() {
            return s.serialize_none();
        }
        s.serialize_str(&B64.encode(v))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            Some(s) if !s.is_empty() => B64.decode(s.as_bytes()).map_err(serde::de::Error::custom),
            _ => Ok(Vec::new()),
        }
    }
}

fn row_to_collected_key(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectedKeyRow> {
    Ok(CollectedKeyRow {
        id: row.get(0)?,
        account_id: row.get(1)?,
        peer_email: row.get(2)?,
        standard: row.get(3)?,
        fingerprint: row.get(4)?,
        public_data: row.get::<_, Option<Vec<u8>>>(5)?.unwrap_or_default(),
        source: row.get(6)?,
        seen_at: row.get(7)?,
    })
}

const COLS: &str =
    "id, account_id, peer_email, standard, fingerprint, public_data, source, seen_at";

/// Stage a discovered key. `seen_at` is stamped server-side via
/// `strftime('%s','now')`. Returns the new row id.
///
/// # Errors
/// Fails when the insert cannot run.
pub fn stage_collected_key(
    store: &CryptoStore,
    account_id: Option<&str>,
    peer_email: Option<&str>,
    standard: Option<&str>,
    fingerprint: Option<&str>,
    public_data: &[u8],
    source: Option<&str>,
) -> Result<i64, String> {
    let conn = store.conn();
    conn.execute(
        "INSERT INTO collected_keys
         (account_id, peer_email, standard, fingerprint, public_data, source, seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s','now'))",
        rusqlite::params![
            account_id,
            peer_email,
            standard,
            fingerprint,
            public_data,
            source
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// Stage a discovered key, skipping the insert when an identical
/// `(account_id, standard, fingerprint, source)` row already exists. The
/// harvest + LDAP staging run on every message open / directory query, so
/// without the dedup a re-opened signed message would append a duplicate
/// staging row each time; `collected_keys` has no UNIQUE constraint, so the
/// guard lives here. Returns `Some(id)` of the new row, or `None` when the
/// key was already staged (a no-op, not an error).
///
/// # Errors
/// Fails when the insert cannot run.
pub fn stage_collected_key_if_absent(
    store: &CryptoStore,
    account_id: Option<&str>,
    peer_email: Option<&str>,
    standard: Option<&str>,
    fingerprint: Option<&str>,
    public_data: &[u8],
    source: Option<&str>,
) -> Result<Option<i64>, String> {
    let exists: Option<i64> = store
        .conn()
        .query_row(
            "SELECT 1 FROM collected_keys
             WHERE account_id IS NOT DISTINCT FROM ?1
               AND standard IS NOT DISTINCT FROM ?2
               AND fingerprint IS NOT DISTINCT FROM ?3
               AND source IS NOT DISTINCT FROM ?4
             LIMIT 1",
            rusqlite::params![account_id, standard, fingerprint, source],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })?;
    if exists.is_some() {
        return Ok(None);
    }
    stage_collected_key(
        store,
        account_id,
        peer_email,
        standard,
        fingerprint,
        public_data,
        source,
    )
    .map(Some)
}

/// List staged keys for a `(account_id, peer_email, standard)` triple,
/// newest first. Any of the filter values may be NULL in the table; the
/// query matches NULLs with `IS NOT DISTINCT FROM` so a `None` filter value
/// matches NULL rows.
///
/// # Errors
/// Fails on a database error.
pub fn list_collected_keys_for_peer(
    store: &CryptoStore,
    account_id: &str,
    peer_email: &str,
    standard: &str,
) -> Result<Vec<CollectedKeyRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM collected_keys
         WHERE account_id IS NOT DISTINCT FROM ?1
           AND peer_email IS NOT DISTINCT FROM ?2
           AND standard IS NOT DISTINCT FROM ?3
         ORDER BY seen_at DESC, id DESC"
    );
    list_rows(
        store,
        &sql,
        rusqlite::params![account_id, peer_email, standard],
    )
}

/// Remove a staged key by id.
///
/// # Errors
/// Fails when the delete cannot run.
pub fn remove_collected_key(store: &CryptoStore, id: i64) -> Result<(), String> {
    store
        .conn()
        .execute(
            "DELETE FROM collected_keys WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear ALL staged keys for an account (the "Clear collected keys"
/// maintenance action). Returns the number of rows removed. Account-scoped:
/// other accounts' staging rows are untouched.
///
/// # Errors
/// Fails when the delete cannot run.
pub fn clear_collected_keys(store: &CryptoStore, account_id: &str) -> Result<u64, String> {
    let n = store
        .conn()
        .execute(
            "DELETE FROM collected_keys WHERE account_id = ?1",
            rusqlite::params![account_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(n as u64)
}

/// List staged keys for `(account_id, standard, fingerprint)` across ALL
/// peer emails / sources, newest first. The promote path looks material up
/// by fingerprint (the trust decision carries the email separately).
///
/// # Errors
/// Fails on a database error.
pub fn list_collected_keys_for_fingerprint(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<Vec<CollectedKeyRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM collected_keys
         WHERE account_id IS NOT DISTINCT FROM ?1
           AND standard IS NOT DISTINCT FROM ?2
           AND fingerprint IS NOT DISTINCT FROM ?3
         ORDER BY seen_at DESC, id DESC"
    );
    list_rows(
        store,
        &sql,
        rusqlite::params![account_id, standard, fingerprint],
    )
}

/// List every staged key for `(account_id, standard)` regardless of peer
/// email / fingerprint / source, newest first. The OpenPGP receive path
/// offers staged certs as embedded-signature verification candidates
/// alongside keystore certs: a discovered-but-unaccepted signer cert
/// verifies instead of degrading to `unknown-key`.
///
/// # Errors
/// Fails on a database error.
pub fn list_collected_keys_for_account(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
) -> Result<Vec<CollectedKeyRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM collected_keys
         WHERE account_id IS NOT DISTINCT FROM ?1
           AND standard IS NOT DISTINCT FROM ?2
         ORDER BY seen_at DESC, id DESC"
    );
    list_rows(store, &sql, rusqlite::params![account_id, standard])
}

/// Remove every staged row for `(account_id, standard, fingerprint)` (all
/// sources). Called after a successful promote so the review list drains.
/// Returns the number of rows removed.
fn delete_collected_keys_for_fingerprint(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<u64, String> {
    let n = store
        .conn()
        .execute(
            "DELETE FROM collected_keys
             WHERE account_id IS NOT DISTINCT FROM ?1
               AND standard IS NOT DISTINCT FROM ?2
               AND fingerprint IS NOT DISTINCT FROM ?3",
            rusqlite::params![account_id, standard, fingerprint],
        )
        .map_err(|e| e.to_string())?;
    Ok(n as u64)
}

/// Map a collected-key `source` to a `crypto_keys.origin` value the CHECK
/// constraint admits. Provenance is preserved verbatim for the discovery
/// sources; anything else (e.g. `'attachment'`) lands as `'imported'`.
fn promote_origin(source: Option<&str>) -> &'static str {
    match source {
        Some("wkd") => "wkd",
        Some("keyserver") => "keyserver",
        Some("autocrypt") => "autocrypt",
        Some("contact") => "contact",
        Some("ldap") => "ldap",
        // The S/MIME harvest bookkeeping source maps to 'harvested': staged
        // intermediates are unvetted chain evidence and must NEVER fall
        // through to 'imported' (which is anchor-eligible by default).
        Some("harvested" | "smime-harvest-intermediate") => "harvested",
        _ => "imported",
    }
}

/// Whether staged S/MIME cert bytes carry `basicConstraints cA:TRUE` (a CA /
/// intermediate cert rather than an end-entity leaf). ANY parse failure or
/// an absent BasicConstraints extension returns `false`; the row then
/// promotes as a regular end-entity `'cert'`, preserving the legacy behavior
/// for non-DER staged blobs.
fn staged_smime_cert_is_ca(public_data: &[u8]) -> bool {
    let Ok((_rem, cert)) = x509_parser::parse_x509_certificate(public_data) else {
        return false;
    };
    cert.basic_constraints()
        .ok()
        .flatten()
        .is_some_and(|bc| bc.value.ca)
}

/// Promote a staged key into `crypto_keys` and clear its staging rows.
///
/// - `email` overrides the collected row's `peer_email` binding when
///   provided (the accept flow always passes the recipient address being
///   accepted; a staged row may carry no peer email).
/// - `key_type` is `'public'` for OpenPGP; for S/MIME it is `'cert'` for
///   end-entity leaf certs and `'intermediate'` when the staged cert carries
///   `basicConstraints cA:TRUE` (intermediates are chain evidence, never
///   trust anchors). `public_data` is the HEX-encoded staged bytes.
/// - **No-downgrade guard:** when a `crypto_keys` row already exists for the
///   fingerprint (ANY origin), the existing row is left COMPLETELY untouched
///   and returned; promotion never rewrites an `imported`/`generated` row.
///   The staging rows are still cleared (the key is in the keystore;
///   staging has served its purpose).
/// - `Err` when no staged row exists AND no keystore row exists: the caller
///   then knows the material is missing entirely.
///
/// # Errors
/// Fails on a database error, or when no staged row AND no keystore row exists for the fingerprint.
pub fn promote_collected_key(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
    email: Option<&str>,
) -> Result<CryptoKeyRow, String> {
    // No-downgrade: an existing keystore row wins untouched.
    if let Some(row) = crate::get_crypto_key_public(store, account_id, standard, fingerprint)? {
        delete_collected_keys_for_fingerprint(store, account_id, standard, fingerprint)?;
        return Ok(row);
    }

    let staged = list_collected_keys_for_fingerprint(store, account_id, standard, fingerprint)?;
    let Some(latest) = staged.into_iter().next() else {
        return Err(format!(
            "no collected key for {account_id}/{standard}/{fingerprint}"
        ));
    };

    // S/MIME CA certs promote as `key_type='intermediate'`, NOT `'cert'`: an
    // intermediate is chain evidence, never a trust anchor. End-entity leaf
    // certs (and unparseable blobs) keep the legacy `'cert'` type.
    let key_type = if standard == "smime" {
        if staged_smime_cert_is_ca(&latest.public_data) {
            "intermediate"
        } else {
            "cert"
        }
    } else {
        "public"
    };
    let origin = promote_origin(latest.source.as_deref());
    let effective_email = email.or(latest.peer_email.as_deref());
    let public_hex = hex::encode(&latest.public_data);
    let id = uuid::Uuid::new_v4().to_string();

    store
        .conn()
        .execute(
            "INSERT INTO crypto_keys (
                id, account_id, standard, key_type, email, fingerprint, public_data,
                private_data_enc, token_serial, token_key_id, origin,
                is_default_sign, is_default_encrypt, created_at, expires_at, policy_json
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                NULL, NULL, NULL, ?8,
                0, 0, strftime('%s','now'), NULL, NULL
            )",
            rusqlite::params![
                id,
                account_id,
                standard,
                key_type,
                effective_email,
                fingerprint,
                public_hex,
                origin
            ],
        )
        .map_err(|e| e.to_string())?;

    delete_collected_keys_for_fingerprint(store, account_id, standard, fingerprint)?;

    crate::get_crypto_key_public(store, account_id, standard, fingerprint)?
        .ok_or_else(|| format!("promoted key {fingerprint} not readable after insert"))
}

fn list_rows(
    store: &CryptoStore,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<CollectedKeyRow>, String> {
    let conn = store.conn();
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params, row_to_collected_key)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "collected_tests.rs"]
mod collected_tests;
