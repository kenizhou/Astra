// SPDX-License-Identifier: GPL-3.0-only
//! Crypto identity key/cert query layer: kylins' `crypto_keys` CRUD, ported
//! from sqlx to rusqlite. Soft private-key blobs are wrapped at rest via
//! [`vault::encrypt_with_aad`] before they touch SQLite, and decrypted
//! in-Rust only via [`get_crypto_key_full`]. The AAD binds
//! `account_id + field name + key version` so a blob captured from one
//! account/field cannot be replayed against another.
//!
//! Two row types are exposed:
//! - [`CryptoKeyRow`]: the public-facing view. No private material; only
//!   `has_private: bool`.
//! - [`CryptoKeyRecord`]: the internal view used by [`upsert_crypto_key`]
//!   and [`get_crypto_key_full`] (in-Rust only). It carries the plaintext
//!   public blob and, optionally, the plaintext private blob the store
//!   encrypts at rest.

use serde::{Deserialize, Serialize};

use crate::store::CryptoStore;
use crate::vault::{decrypt_with_aad, encrypt_with_aad};

/// AAD key-version constant. Bumped into the AAD string so a future rotation
/// can migrate blobs without ambiguity: `aad = "astra:{account_id}:private_key:{ver}"`.
/// (kylins used the `kylins:` prefix; this store is fresh, so the Astra
/// prefix is safe.)
const PRIVATE_KEY_AAD_VERSION: u8 = 1;

/// The public-facing crypto key row. Carries NO private key material, only
/// [`CryptoKeyRow::has_private`] indicating whether a soft private blob
/// exists at rest. JSON keys are camelCase for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CryptoKeyRow {
    /// Server-generated uuid.
    pub id: String,
    /// The account this key belongs to (every query is scoped by it).
    pub account_id: String,
    /// `'openpgp' | 'smime' | 'sm'`.
    pub standard: String,
    /// `'public' | 'private' | 'cert' | 'intermediate'`.
    pub key_type: String,
    /// The email binding the send side resolves recipients by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Primary fingerprint (OpenPGP) / SHA-1-of-SPKI hex (S/MIME).
    pub fingerprint: String,
    /// How the key arrived (`'generated'`, `'imported'`, `'harvested'`, ...).
    pub origin: String,
    /// The account's default signing key for its standard.
    pub is_default_sign: bool,
    /// The account's default encryption key for its standard.
    pub is_default_encrypt: bool,
    /// Unix-seconds string, stamped server-side at insert.
    pub created_at: String,
    /// Unix-seconds string, when the key material expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// X.509 display metadata: subject CN + validity start, computed from
    /// the cert blob at write time. NULL for OpenPGP rows and unparseable
    /// blobs; the UI falls back to the fingerprint display. `expires_at`
    /// already carries the validity END.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_cn: Option<String>,
    /// Unix-seconds string: the cert validity start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    /// `true` when `private_data_enc IS NOT NULL` (a soft private blob
    /// exists) or the key lives on a token.
    pub has_private: bool,
    /// The hardware token's serial, when the key is token-backed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_serial: Option<String>,
    /// The key's id on the token, when token-backed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_key_id: Option<String>,
}

/// The internal crypto key record. Carries the plaintext public blob and,
/// optionally, the plaintext private blob. Used as the [`upsert_crypto_key`]
/// input and as the [`get_crypto_key_full`] return. **Never crosses an IPC
/// boundary**: the decrypting read is in-Rust only.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CryptoKeyRecord {
    /// The public-facing row fields.
    #[serde(flatten)]
    pub row: CryptoKeyRow,
    /// Armored PGP / PEM / DER (hex) public material.
    pub public_data: String,
    /// Plaintext armored/DER private material; `None` for public-only/token
    /// keys. The store encrypts this at rest via [`encrypt_with_aad`] before
    /// writing, and decrypts on read in [`get_crypto_key_full`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_data: Option<String>,
    /// Per-key metadata JSON (algorithm + usage ride here; no db columns).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_json: Option<String>,
}

/// Build the AAD bound into the private-key wrapping. The same string is
/// recomputed on decrypt so a blob lifted from one account/field/version
/// fails AEAD verification elsewhere.
pub(crate) fn private_aad(account_id: &str) -> String {
    format!("astra:{account_id}:private_key:{PRIVATE_KEY_AAD_VERSION}")
}

/// Compute the `(subject_cn, not_before, email)` display fields from a cert
/// DER blob, shared by every `crypto_keys` write path. Parse failure yields
/// all-`None` (the row keeps NULLs and the UI falls back to the fingerprint
/// display).
pub(crate) fn cert_summary_fields(der: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    match astra_crypto_smime::cert_summary_of_der(der) {
        Ok(s) => (s.subject_cn, Some(s.not_before_unix.to_string()), s.email),
        Err(_) => (None, None, None),
    }
}

type KeyRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
    Option<String>,
);

fn tuple_to_crypto_key(t: KeyRow) -> CryptoKeyRow {
    CryptoKeyRow {
        id: t.0,
        account_id: t.1,
        standard: t.2,
        key_type: t.3,
        email: t.4,
        fingerprint: t.5,
        origin: t.6,
        is_default_sign: t.7 != 0,
        is_default_encrypt: t.8 != 0,
        created_at: t.9,
        expires_at: t.10,
        subject_cn: t.11,
        not_before: t.12,
        has_private: t.13 != 0,
        token_serial: t.14,
        token_key_id: t.15,
    }
}

/// Insert or update a crypto key. If [`CryptoKeyRecord::private_data`] is
/// `Some(pt)`, it is wrapped via [`encrypt_with_aad`] with the account-bound
/// AAD and stored in `private_data_enc`; otherwise `private_data_enc` is
/// NULL. On conflict (same `account_id + standard + fingerprint`) the `id` +
/// `created_at` + `origin` are preserved; all other fields update.
///
/// # Errors
/// Fails when the private blob cannot be wrapped or the upsert cannot run.
pub fn upsert_crypto_key(store: &CryptoStore, rec: &CryptoKeyRecord) -> Result<(), String> {
    let id = if rec.row.id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        rec.row.id.clone()
    };

    // Encrypt the private blob at rest if present.
    let private_data_enc: Option<String> = match rec.private_data.as_deref() {
        Some(pt) if !pt.is_empty() => {
            let aad = private_aad(&rec.row.account_id);
            Some(encrypt_with_aad(pt.as_bytes(), aad.as_bytes()).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    // Fill X.509 display metadata from the cert blob for any field the
    // caller left unset. `public_data` is hex(DER) for X.509 rows; OpenPGP
    // blobs and PEM fakes fail hex-decode/parse and stay NULL, which the UI
    // treats as "fall back to the fingerprint display". A missing email is
    // likewise filled from the cert SAN.
    let mut subject_cn = rec.row.subject_cn.clone();
    let mut not_before = rec.row.not_before.clone();
    let mut email = rec.row.email.clone();
    if subject_cn.is_none() || not_before.is_none() || email.is_none() {
        let (sum_cn, sum_nb, sum_email) = hex::decode(&rec.public_data)
            .map(|der| cert_summary_fields(&der))
            .unwrap_or_default();
        if subject_cn.is_none() {
            subject_cn = sum_cn;
        }
        if not_before.is_none() {
            not_before = sum_nb;
        }
        if email.is_none() {
            email = sum_email;
        }
    }

    // COALESCE in the conflict clause marks fields where NULL means "not
    // supplied" (never an intentional clear): a re-upsert carrying an
    // unparseable blob or no private material must NOT wipe the previously
    // computed display metadata / email / stored soft private key.
    store
        .conn()
        .execute(
            "INSERT INTO crypto_keys (
                id, account_id, standard, key_type, email, fingerprint, public_data,
                private_data_enc, token_serial, token_key_id, origin,
                is_default_sign, is_default_encrypt, created_at, expires_at, policy_json,
                subject_cn, not_before
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                ?12, ?13, strftime('%s','now'), ?14, ?15,
                ?16, ?17
            )
            ON CONFLICT(account_id, standard, fingerprint) DO UPDATE SET
                key_type = excluded.key_type,
                email = COALESCE(excluded.email, email),
                public_data = excluded.public_data,
                private_data_enc = COALESCE(excluded.private_data_enc, private_data_enc),
                token_serial = excluded.token_serial,
                token_key_id = excluded.token_key_id,
                is_default_sign = excluded.is_default_sign,
                is_default_encrypt = excluded.is_default_encrypt,
                expires_at = excluded.expires_at,
                policy_json = excluded.policy_json,
                subject_cn = COALESCE(excluded.subject_cn, subject_cn),
                not_before = COALESCE(excluded.not_before, not_before)",
            rusqlite::params![
                id,
                rec.row.account_id,
                rec.row.standard,
                rec.row.key_type,
                email,
                rec.row.fingerprint,
                rec.public_data,
                private_data_enc,
                rec.row.token_serial,
                rec.row.token_key_id,
                rec.row.origin,
                i64::from(rec.row.is_default_sign),
                i64::from(rec.row.is_default_encrypt),
                rec.row.expires_at,
                rec.policy_json,
                subject_cn,
                not_before,
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// `has_private` means "a private OPERATION is available": either a soft
/// private blob exists at rest OR the key lives on a hardware token (the
/// private op is delegated to the device). This single semantic makes token
/// keys participate everywhere candidate selection already filters on
/// `has_private`.
const PUBLIC_COLS: &str = "id, account_id, standard, key_type, email, fingerprint, origin, \
     is_default_sign, is_default_encrypt, created_at, expires_at, \
     subject_cn, not_before, \
     (private_data_enc IS NOT NULL OR token_serial IS NOT NULL) AS has_private, \
     token_serial, token_key_id";

/// Return the public-facing row for a key identified by `(account_id,
/// standard, fingerprint)`, or `None`. Never exposes private material.
/// Account-scoped: a same-fingerprint row under another account is NOT
/// returned (prevents cross-account signer-metadata leaks).
///
/// # Errors
/// Fails on a database error.
pub fn get_crypto_key_public(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<Option<CryptoKeyRow>, String> {
    let sql = format!("SELECT {PUBLIC_COLS} FROM crypto_keys WHERE account_id = ?1 AND standard = ?2 AND fingerprint = ?3");
    store
        .conn()
        .query_row(
            &sql,
            rusqlite::params![account_id, standard, fingerprint],
            |row| {
                Ok(tuple_to_crypto_key((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                )))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// Like [`get_crypto_key_public`] but also returns the stored `public_data`
/// blob (hex-encoded armored/binary public material). The private blob is
/// NOT touched: unlike [`get_crypto_key_full`] this performs no decryption.
/// Account-scoped identically to [`get_crypto_key_public`].
///
/// # Errors
/// Fails on a database error.
pub fn get_crypto_key_public_with_blob(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<Option<(CryptoKeyRow, String)>, String> {
    let sql = format!(
        "SELECT {PUBLIC_COLS}, public_data FROM crypto_keys WHERE account_id = ?1 AND standard = ?2 AND fingerprint = ?3"
    );
    store
        .conn()
        .query_row(
            &sql,
            rusqlite::params![account_id, standard, fingerprint],
            |row| {
                let key = tuple_to_crypto_key((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                ));
                let public_data: String = row.get(16)?;
                Ok((key, public_data))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// The full-record column list shared by [`get_crypto_key_full`] and
/// `get_crypto_key_by_token`.
pub(crate) const FULL_COLS: &str = "id, account_id, standard, key_type, email, fingerprint, \
     public_data, private_data_enc, token_serial, token_key_id, origin, \
     is_default_sign, is_default_encrypt, created_at, expires_at, policy_json, \
     subject_cn, not_before";

/// Map a full-record row (columns in [`FULL_COLS`] order) to a
/// [`CryptoKeyRecord`], decrypting the private blob when present. The AAD
/// derives from the ROW's own account id, so a wrong-account blob fails AEAD
/// verification.
pub(crate) fn full_row_to_record(
    row: &rusqlite::Row<'_>,
    decrypt_private: bool,
) -> rusqlite::Result<CryptoKeyRecord> {
    let row_account_id: String = row.get(1)?;
    let enc_hex: Option<String> = row.get(7)?;
    let private_data = if decrypt_private {
        match enc_hex.as_deref().filter(|s| !s.is_empty()) {
            Some(enc) => {
                let aad = private_aad(&row_account_id);
                let bytes = decrypt_with_aad(enc, aad.as_bytes()).map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?;
                Some(String::from_utf8(bytes).map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?)
            }
            None => None,
        }
    } else {
        None
    };
    Ok(CryptoKeyRecord {
        row: CryptoKeyRow {
            id: row.get(0)?,
            account_id: row_account_id,
            standard: row.get(2)?,
            key_type: row.get(3)?,
            email: row.get(4)?,
            fingerprint: row.get(5)?,
            origin: row.get(10)?,
            is_default_sign: row.get::<_, i64>(11)? != 0,
            is_default_encrypt: row.get::<_, i64>(12)? != 0,
            created_at: row.get(13)?,
            expires_at: row.get(14)?,
            subject_cn: row.get(16)?,
            not_before: row.get(17)?,
            has_private: enc_hex.is_some() || row.get::<_, Option<String>>(8)?.is_some(),
            token_serial: row.get(8)?,
            token_key_id: row.get(9)?,
        },
        public_data: row.get(6)?,
        private_data,
        policy_json: row.get(15)?,
    })
}

/// Return the full record (incl. decrypted private blob) for `(account_id,
/// standard, fingerprint)`, or `None`. **In-Rust only, never a command.**
/// Recomputes the same AAD used at write time and decrypts via
/// [`decrypt_with_aad`]; a wrong AAD (different account/version) fails AEAD
/// verification.
///
/// Account-scoped: the `account_id = ?` filter is the fail-closed floor for
/// PRIVATE material; without it, a same-fingerprint row under another
/// account resolved AND decrypted, leaking that account's private key. A
/// miss is `None`, never another account's key.
///
/// # Errors
/// Fails on a database error or when the stored private blob fails AEAD verification.
pub fn get_crypto_key_full(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<Option<CryptoKeyRecord>, String> {
    let sql = format!(
        "SELECT {FULL_COLS} FROM crypto_keys WHERE account_id = ?1 AND standard = ?2 AND fingerprint = ?3"
    );
    store
        .conn()
        .query_row(
            &sql,
            rusqlite::params![account_id, standard, fingerprint],
            |row| full_row_to_record(row, true),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// List public-facing rows for `(standard, email)`. Ordered by fingerprint
/// for deterministic output. Never exposes private material.
///
/// # Errors
/// Fails on a database error.
pub fn list_crypto_keys_for_email(
    store: &CryptoStore,
    standard: &str,
    email: &str,
) -> Result<Vec<CryptoKeyRow>, String> {
    let sql = format!(
        "SELECT {PUBLIC_COLS} FROM crypto_keys WHERE standard = ?1 AND email = ?2 ORDER BY fingerprint"
    );
    list_public_rows(store, &sql, rusqlite::params![standard, email])
}

/// List public-facing rows for `(account_id, standard)`. Ordered by
/// fingerprint. Never exposes private material.
///
/// # Errors
/// Fails on a database error.
pub fn list_crypto_keys_for_account(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
) -> Result<Vec<CryptoKeyRow>, String> {
    let sql = format!(
        "SELECT {PUBLIC_COLS} FROM crypto_keys WHERE account_id = ?1 AND standard = ?2 ORDER BY fingerprint"
    );
    list_public_rows(store, &sql, rusqlite::params![account_id, standard])
}

fn list_public_rows(
    store: &CryptoStore,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<CryptoKeyRow>, String> {
    let conn = store.conn();
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params, |row| {
            Ok(tuple_to_crypto_key((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
            )))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod keys_tests;
