// SPDX-License-Identifier: GPL-3.0-only
//! The per-account crypto policy (`accounts.crypto_policy_json`): kylins'
//! discovery-config parse, send-side half. `send_autocrypt_headers` and
//! `protect_subject` default ON when the key is absent (Thunderbird's
//! `mail.identity.default` posture); unknown JSON is ignored defensively.

use serde_json::Value;

use crate::store::CryptoStore;

/// Read the account's raw policy JSON (`None` when the account is unknown
/// or the column is NULL).
fn read_policy_json(store: &CryptoStore, account_id: &str) -> Result<Option<String>, String> {
    let cell: Option<Option<String>> = store
        .conn()
        .query_row(
            "SELECT crypto_policy_json FROM accounts WHERE id = ?1",
            rusqlite::params![account_id],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })?;
    Ok(cell.flatten())
}

fn parse_flag(json: Option<&str>, key: &str, default: bool) -> bool {
    json.and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|root| root.get(key).and_then(Value::as_bool))
        .unwrap_or(default)
}

/// Whether the account sends `Autocrypt:` headers on outgoing mail (default
/// ON when the key is absent: the Level 1 default posture).
///
/// # Errors
/// Fails on a database error.
pub fn get_send_autocrypt_headers(store: &CryptoStore, account_id: &str) -> Result<bool, String> {
    let json = read_policy_json(store, account_id)?;
    Ok(parse_flag(json.as_deref(), "send_autocrypt_headers", true))
}

/// Whether the account protects the Subject header on outgoing ENCRYPTED
/// mail (default ON when the key is absent: mirrors Thunderbird's
/// `mail.identity.default.protectSubject` default). Read by the send gate's
/// encrypt branch; sign-only mail always keeps the cleartext subject.
///
/// # Errors
/// Fails on a database error.
pub fn get_protect_subject(store: &CryptoStore, account_id: &str) -> Result<bool, String> {
    let json = read_policy_json(store, account_id)?;
    Ok(parse_flag(json.as_deref(), "protect_subject", true))
}

/// Write one policy flag via read-modify-write, preserving sibling keys.
///
/// # Errors
/// Fails on a database error.
pub fn set_policy_flag(
    store: &CryptoStore,
    account_id: &str,
    key: &str,
    value: bool,
) -> Result<(), String> {
    let json = read_policy_json(store, account_id)?;
    let mut root: Value = json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    if !root.is_object() {
        root = Value::Object(serde_json::Map::new());
    }
    root[key] = Value::Bool(value);
    let text = serde_json::to_string(&root).map_err(|e| e.to_string())?;
    store
        .conn()
        .execute(
            "UPDATE accounts SET crypto_policy_json = ?1 WHERE id = ?2",
            rusqlite::params![text, account_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
#[path = "crypto_policy_tests.rs"]
mod crypto_policy_tests;
