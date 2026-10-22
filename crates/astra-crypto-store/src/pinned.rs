// SPDX-License-Identifier: GPL-3.0-only
//! Pinned (preferred) key storage: kylins' `pinned_keys.rs` ported. One row
//! per normalised email in `contact_pinned_keys`, the JSON array of
//! `{standard, fingerprint}` objects, one preferred key per standard per
//! contact. Recipient resolution prefers the pinned fingerprint when
//! multiple candidate keys exist for one email. The JSON shape tolerates a
//! legacy `data` member: unknown members are ignored on read and dropped on
//! write.

use serde::{Deserialize, Serialize};

use crate::store::CryptoStore;

/// One pinned-key entry. Mirrors the `{standard, fingerprint}` objects
/// stored in `contact_pinned_keys.pinned_keys_json`; JSON keys are
/// camelCase for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PinnedKey {
    /// `'openpgp' | 'smime' | 'sm'`.
    pub standard: String,
    /// The preferred key's fingerprint for this standard.
    pub fingerprint: String,
}

/// Trim + lowercase an email address.
fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Read the raw `pinned_keys_json` blob for an email, or `None` when no row
/// exists.
fn read_pinned_blob(store: &CryptoStore, email: &str) -> Result<Option<String>, String> {
    let normalized = normalize_email(email);
    store
        .conn()
        .query_row(
            "SELECT pinned_keys_json FROM contact_pinned_keys WHERE email = ?1",
            rusqlite::params![normalized],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// Parse the pinned-key list out of a blob. Malformed blobs and entries
/// missing `standard`/`fingerprint` are skipped (never error the read: a
/// corrupt pin must not break contact-key listing).
fn parse_pinned(blob: Option<&str>) -> Vec<PinnedKey> {
    let Some(text) = blob else { return Vec::new() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| {
            let standard = entry.get("standard")?.as_str()?.to_string();
            let fingerprint = entry.get("fingerprint")?.as_str()?.to_string();
            if standard.is_empty() || fingerprint.is_empty() {
                return None;
            }
            Some(PinnedKey {
                standard,
                fingerprint,
            })
        })
        .collect()
}

/// Upsert the pinned-key list for an email (one row per email).
fn write_pinned(store: &CryptoStore, email: &str, pins: &[PinnedKey]) -> Result<(), String> {
    let normalized = normalize_email(email);
    let blob = serde_json::to_string(pins).map_err(|e| e.to_string())?;
    store
        .conn()
        .execute(
            "INSERT INTO contact_pinned_keys (email, pinned_keys_json) VALUES (?1, ?2)
             ON CONFLICT(email) DO UPDATE SET pinned_keys_json = excluded.pinned_keys_json",
            rusqlite::params![normalized, blob],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// List the pinned keys for a contact email (empty when none).
///
/// # Errors
/// Fails on a database error.
pub fn list_pinned_keys(store: &CryptoStore, email: &str) -> Result<Vec<PinnedKey>, String> {
    let blob = read_pinned_blob(store, email)?;
    Ok(parse_pinned(blob.as_deref()))
}

/// Pin `fingerprint` as the preferred key for `(email, standard)`. One pin
/// per standard: an existing pin for the same standard is replaced; pins for
/// other standards are preserved. Creates the row when absent.
///
/// # Errors
/// Fails when `standard` or `fingerprint` is blank, or on a database error.
pub fn set_pinned_key(
    store: &CryptoStore,
    email: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<(), String> {
    if standard.trim().is_empty() || fingerprint.trim().is_empty() {
        return Err("set_pinned_key: standard and fingerprint must be non-empty".into());
    }
    let blob = read_pinned_blob(store, email)?;
    let mut pins = parse_pinned(blob.as_deref());
    pins.retain(|p| p.standard != standard);
    pins.push(PinnedKey {
        standard: standard.to_string(),
        fingerprint: fingerprint.to_string(),
    });
    write_pinned(store, email, &pins)
}

/// Remove the pin for `(email, standard, fingerprint)`. Idempotent: a
/// missing row / missing pin is a no-op.
///
/// # Errors
/// Fails on a database error.
pub fn remove_pinned_key(
    store: &CryptoStore,
    email: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let blob = read_pinned_blob(store, email)?;
    let pins = parse_pinned(blob.as_deref());
    let filtered: Vec<PinnedKey> = pins
        .into_iter()
        .filter(|p| !(p.standard == standard && p.fingerprint == fingerprint))
        .collect();
    // No row (None blob) means nothing to update.
    if blob.is_none() {
        return Ok(());
    }
    write_pinned(store, email, &filtered)
}

#[cfg(test)]
#[path = "pinned_tests.rs"]
mod pinned_tests;
