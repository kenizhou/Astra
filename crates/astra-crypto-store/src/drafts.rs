// SPDX-License-Identifier: GPL-3.0-only
//! The encrypted draft mirror: a durable Astra-held copy of each compose
//! draft, wrapped at rest through [`vault::encrypt_with_aad`] before it
//! touches SQLite. The engine persists its own draft copies as plaintext and
//! cannot be changed, so this mirror is the copy resume prefers; the payload
//! is opaque bytes here (the store never names the engine's `Draft`), and the
//! AAD binds `account_id + draft_key + version` so a blob lifted from one
//! account or draft fails AEAD verification anywhere else.

use crate::store::CryptoStore;
use crate::vault::{decrypt_with_aad, encrypt_with_aad, CryptoVaultError};

/// AAD key-version constant, bumped into the AAD string so a future rotation
/// can migrate blobs without ambiguity:
/// `aad = "astra:{account_id}:draft:{draft_key}:{ver}"`.
const DRAFT_AAD_VERSION: u8 = 1;

/// The typed mirror error: a vault refusal (keyring unavailable, AEAD failure)
/// or a store failure. The stack's discipline is fail closed, so a caller
/// treats any error as "the mirror cannot be trusted", never a plaintext
/// fallback.
#[derive(Debug, thiserror::Error)]
pub enum DraftMirrorError {
    /// The vault could not wrap or unwrap the blob.
    #[error("draft vault: {0}")]
    Vault(#[from] CryptoVaultError),
    /// The store could not read or write the row.
    #[error("draft store: {0}")]
    Store(String),
}

/// Build the AAD bound into the draft wrapping. The same string is recomputed
/// on decrypt so a blob lifted from one account/draft/version fails AEAD
/// verification elsewhere.
pub(crate) fn draft_aad(account_id: &str, draft_key: &str) -> String {
    format!("astra:{account_id}:draft:{draft_key}:{DRAFT_AAD_VERSION}")
}

/// Wrap `plaintext` under the account- and draft-bound AAD and upsert the
/// mirror row. A re-save of the same draft upserts the same key.
///
/// # Errors
/// Fails when the vault cannot wrap the blob or the upsert cannot run.
pub fn put_encrypted_draft(
    store: &CryptoStore,
    account_id: &str,
    draft_key: &str,
    plaintext: &[u8],
) -> Result<(), DraftMirrorError> {
    let aad = draft_aad(account_id, draft_key);
    let blob = encrypt_with_aad(plaintext, aad.as_bytes())?;
    store
        .conn()
        .execute(
            "INSERT INTO encrypted_drafts (account_id, draft_key, body_enc, updated_at)
             VALUES (?1, ?2, ?3, strftime('%s','now'))
             ON CONFLICT(account_id, draft_key) DO UPDATE SET
                 body_enc = excluded.body_enc,
                 updated_at = excluded.updated_at",
            rusqlite::params![account_id, draft_key, blob],
        )
        .map_err(|e| DraftMirrorError::Store(e.to_string()))?;
    Ok(())
}

/// Read the mirror row and unwrap it in-Rust. `Ok(None)` when no row exists
/// (the draft was never mirrored); `Ok(Some(bytes))` with the plaintext; a
/// typed error when the row exists but cannot be decrypted.
///
/// # Errors
/// Fails on a store error or when the stored blob fails AEAD verification.
pub fn get_encrypted_draft(
    store: &CryptoStore,
    account_id: &str,
    draft_key: &str,
) -> Result<Option<Vec<u8>>, DraftMirrorError> {
    let blob: Option<String> = store
        .conn()
        .query_row(
            "SELECT body_enc FROM encrypted_drafts WHERE account_id = ?1 AND draft_key = ?2",
            rusqlite::params![account_id, draft_key],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(DraftMirrorError::Store(other.to_string())),
        })?;
    let Some(blob) = blob else {
        return Ok(None);
    };
    let aad = draft_aad(account_id, draft_key);
    let bytes = decrypt_with_aad(&blob, aad.as_bytes())?;
    Ok(Some(bytes))
}

/// Remove the mirror row. A missing row is already gone, so the delete is ok.
///
/// # Errors
/// Fails when the delete cannot run.
pub fn delete_encrypted_draft(
    store: &CryptoStore,
    account_id: &str,
    draft_key: &str,
) -> Result<(), DraftMirrorError> {
    store
        .conn()
        .execute(
            "DELETE FROM encrypted_drafts WHERE account_id = ?1 AND draft_key = ?2",
            rusqlite::params![account_id, draft_key],
        )
        .map_err(|e| DraftMirrorError::Store(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod drafts_tests;
