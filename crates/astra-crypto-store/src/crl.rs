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

/// Return the raw DER bytes of every cached CRL (the whole cache, no URL
/// filter). The SP4c-2 receive orchestrator feeds these to
/// `SmimeBackend::verify_with_context`'s `crls` argument. Cache only: there is
/// no fetcher (SP4c-3), so an empty cache means revocation is unchecked in
/// practice. Rows are returned in `crl_url` order for deterministic output.
///
/// # Errors
/// Fails on a database error.
pub fn list_crls(store: &CryptoStore) -> Result<Vec<Vec<u8>>, String> {
    let conn = store.conn();
    let mut stmt = conn
        .prepare("SELECT crl_der FROM crl_cache ORDER BY crl_url")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for der in rows {
        out.push(der.map_err(|e| e.to_string())?);
    }
    Ok(out)
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

/// The material a [`CrlFetcher`] returns for one distribution-point URL: the
/// issuer-signed CRL DER plus the cache metadata parsed from it. The host
/// implementation fills the metadata via the smime crate's CRL parser; the
/// store upserts it verbatim (it never parses CRL DER itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedCrl {
    /// Raw DER bytes of the issuer-signed CRL.
    pub crl_der: Vec<u8>,
    /// The issuing CA's distinguished name, for display.
    pub issuer_dn: Option<String>,
    /// The CRL `nextUpdate` as an epoch-seconds string (the freshness key).
    pub next_update: Option<String>,
}

/// The CRL fetch port (SP4c-3b D2, the `SendCryptoPolicy` precedent): the store
/// defines it, the host implements it, so the store keeps its dependency
/// direction and never depends on astra-app. A fetch is fail-soft: an `Err`
/// degrades the cert to unchecked revocation and NEVER fails the message open.
/// The implementation must bound the fetch (short timeout, response-size cap,
/// http(s) only) because the distribution-point URL is untrusted input.
#[async_trait::async_trait]
pub trait CrlFetcher: Send + Sync + std::fmt::Debug {
    /// Fetch the CRL for `crl_url`. `Err` on any transport, scheme, size, or
    /// parse failure; the caller degrades to unchecked.
    async fn fetch(&self, crl_url: &str) -> Result<FetchedCrl, String>;
}

/// The offline no-op fetcher: every fetch fails, so the receive path uses only
/// what is already cached and revocation degrades to unchecked. The default
/// for callers with no host fetcher (tests, the offline reading path).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopCrlFetcher;

#[async_trait::async_trait]
impl CrlFetcher for NoopCrlFetcher {
    async fn fetch(&self, _crl_url: &str) -> Result<FetchedCrl, String> {
        Err("CRL fetching is unavailable (offline)".to_string())
    }
}

/// Whether a cached CRL is still fresh at `now_epoch`. A row with no
/// `next_update` has no known expiry and is kept (mirrors `prune_stale_crls`,
/// which never evicts a NULL `next_update`). Otherwise the row is fresh while
/// `next_update > now_epoch`; an unparseable value is treated as stale so the
/// next open refetches.
fn crl_is_fresh(next_update: Option<&str>, now_epoch: &str) -> bool {
    let Some(nu) = next_update else {
        return true;
    };
    match (nu.parse::<i64>(), now_epoch.parse::<i64>()) {
        (Ok(nu), Ok(now)) => nu > now,
        _ => false,
    }
}

/// Fetch-or-cache one distribution-point URL (SP4c-3b D2). Use the cached CRL
/// when it is fresh; otherwise fetch through the port and upsert. Fail-soft
/// throughout: a fetch failure, a non-CRL body, or a store error leaves the
/// cache as-is and never errors, so the message open is never blocked. The
/// chain validates as-of signing time, so a freshly fetched CRL is still
/// checked against the signing instant.
pub async fn fetch_or_cache_crl(
    store: &CryptoStore,
    fetcher: &dyn CrlFetcher,
    crl_url: &str,
    now_epoch: &str,
) {
    // A fresh cached CRL stands: no fetch.
    if let Ok(Some(row)) = get_crl(store, crl_url) {
        if crl_is_fresh(row.next_update.as_deref(), now_epoch) {
            return;
        }
    }
    // Stale or missing: fetch through the port. Any failure degrades to
    // unchecked (the cache stands as-is) and never blocks the open.
    let Ok(crl) = fetcher.fetch(crl_url).await else {
        return;
    };
    let row = CrlCacheRow {
        crl_url: crl_url.to_string(),
        crl_der: crl.crl_der,
        issuer_dn: crl.issuer_dn,
        next_update: crl.next_update,
        fetched_at: now_epoch.to_string(),
    };
    // A store write failure is likewise fail-soft: the open proceeds with
    // whatever the cache already holds.
    let _ = upsert_crl(store, &row);
}

#[cfg(test)]
#[path = "crl_tests.rs"]
mod crl_tests;
