// SPDX-License-Identifier: GPL-3.0-only
//! CRL (Certificate Revocation List) cache query layer: kylins'
//! `crl_cache.rs` ported. Rows are keyed by distribution-point URL;
//! `next_update` drives [`prune_stale_crls`] which evicts expired entries.

use serde::{Deserialize, Serialize};

use crate::store::CryptoStore;

/// A cached CRL row. JSON keys are camelCase for the IPC boundary. The DER
/// blob is stored verbatim (the issuer's signed CRL bytes); validation and
/// signature checking happen in the receive pipeline, not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CrlCacheRow {
    /// The CRL distribution-point URL (the primary key).
    pub crl_url: String,
    /// Raw DER bytes of the issuer-signed CRL.
    pub crl_der: Vec<u8>,
    /// The issuing CA's distinguished name, for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_dn: Option<String>,
    /// Epoch-seconds string. A row is stale once `now >= next_update`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_update: Option<String>,
    /// Epoch-seconds string (`strftime('%s','now')` at write time).
    pub fetched_at: String,
}

/// Insert or replace a cached CRL. On conflict (same `crl_url`) every field
/// updates: the latest fetch always wins. Caller supplies `fetched_at`
/// (convention: `strftime('%s','now')` string epoch seconds).
///
/// # Errors
/// Fails when the upsert cannot run.
pub fn upsert_crl(store: &CryptoStore, row: &CrlCacheRow) -> Result<(), String> {
    store
        .conn()
        .execute(
            "INSERT INTO crl_cache (crl_url, crl_der, issuer_dn, next_update, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(crl_url) DO UPDATE SET
                crl_der = excluded.crl_der,
                issuer_dn = excluded.issuer_dn,
                next_update = excluded.next_update,
                fetched_at = excluded.fetched_at",
            rusqlite::params![
                row.crl_url,
                row.crl_der,
                row.issuer_dn,
                row.next_update,
                row.fetched_at
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Read a cached CRL by its distribution-point URL, or `None` if not cached.
///
/// # Errors
/// Fails on a database error.
pub fn get_crl(store: &CryptoStore, crl_url: &str) -> Result<Option<CrlCacheRow>, String> {
    store
        .conn()
        .query_row(
            "SELECT crl_url, crl_der, issuer_dn, next_update, fetched_at
             FROM crl_cache WHERE crl_url = ?1",
            rusqlite::params![crl_url],
            |row| {
                Ok(CrlCacheRow {
                    crl_url: row.get(0)?,
                    crl_der: row.get(1)?,
                    issuer_dn: row.get(2)?,
                    next_update: row.get(3)?,
                    fetched_at: row.get(4)?,
                })
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// Delete cached CRLs whose `next_update` is strictly less than `now_epoch`
/// (string comparison on epoch-second strings is correct for equal-length
/// numeric strings). Returns the number of rows deleted. Idempotent.
///
/// # Errors
/// Fails when the delete cannot run.
pub fn prune_stale_crls(store: &CryptoStore, now_epoch: &str) -> Result<u64, String> {
    let n = store
        .conn()
        .execute(
            "DELETE FROM crl_cache WHERE next_update IS NOT NULL AND next_update < ?1",
            rusqlite::params![now_epoch],
        )
        .map_err(|e| e.to_string())?;
    Ok(n as u64)
}

#[cfg(test)]
#[path = "crl_tests.rs"]
mod crl_tests;
