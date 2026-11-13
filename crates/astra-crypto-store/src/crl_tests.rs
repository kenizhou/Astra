// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the CRL cache. Ported from kylins' crl_cache tests.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    CryptoStore::open_in_memory().expect("open in-memory store")
}

#[test]
fn upsert_then_get_round_trips() {
    let store = test_store();
    let row = CrlCacheRow {
        crl_url: "http://ca.example.com/crl.der".to_string(),
        crl_der: b"fake-der-bytes".to_vec(),
        issuer_dn: Some("CN=Test CA".to_string()),
        next_update: Some("1800000000".to_string()),
        fetched_at: "1770000000".to_string(),
    };
    upsert_crl(&store, &row).unwrap();

    let got = get_crl(&store, "http://ca.example.com/crl.der")
        .unwrap()
        .expect("row present");
    assert_eq!(got, row);
    assert_eq!(got.crl_der, b"fake-der-bytes");
}

#[test]
fn upsert_replaces_on_conflict() {
    let store = test_store();
    let mut row = CrlCacheRow {
        crl_url: "http://ca.example.com/crl.der".to_string(),
        crl_der: b"v1".to_vec(),
        issuer_dn: Some("CN=Test CA".to_string()),
        next_update: Some("1800000000".to_string()),
        fetched_at: "1770000000".to_string(),
    };
    upsert_crl(&store, &row).unwrap();

    // Re-fetch: new DER + advanced next_update + new fetched_at.
    row.crl_der = b"v2-refreshed".to_vec();
    row.next_update = Some("1810000000".to_string());
    row.fetched_at = "1780000000".to_string();
    upsert_crl(&store, &row).unwrap();

    let got = get_crl(&store, "http://ca.example.com/crl.der")
        .unwrap()
        .expect("row present");
    assert_eq!(got.crl_der, b"v2-refreshed");
    assert_eq!(got.next_update.as_deref(), Some("1810000000"));
    assert_eq!(got.fetched_at, "1780000000");
}

#[test]
fn get_returns_none_when_absent() {
    let store = test_store();
    assert!(get_crl(&store, "http://nope.example.com/crl.der")
        .unwrap()
        .is_none());
}

#[test]
fn prune_deletes_only_stale_rows() {
    let store = test_store();

    // Two CRLs: one stale (next_update before now), one fresh.
    let stale = CrlCacheRow {
        crl_url: "http://ca.example.com/stale.der".to_string(),
        crl_der: b"stale".to_vec(),
        issuer_dn: None,
        next_update: Some("1700000000".to_string()),
        fetched_at: "1690000000".to_string(),
    };
    let fresh = CrlCacheRow {
        crl_url: "http://ca.example.com/fresh.der".to_string(),
        crl_der: b"fresh".to_vec(),
        issuer_dn: None,
        next_update: Some("1900000000".to_string()),
        fetched_at: "1770000000".to_string(),
    };
    // A row with NULL next_update is NEVER pruned (no expiry known).
    let no_expiry = CrlCacheRow {
        crl_url: "http://ca.example.com/no-expiry.der".to_string(),
        crl_der: b"unknown".to_vec(),
        issuer_dn: None,
        next_update: None,
        fetched_at: "1770000000".to_string(),
    };
    upsert_crl(&store, &stale).unwrap();
    upsert_crl(&store, &fresh).unwrap();
    upsert_crl(&store, &no_expiry).unwrap();

    let deleted = prune_stale_crls(&store, "1800000000").unwrap();
    assert_eq!(deleted, 1, "only the stale row is deleted");

    assert!(get_crl(&store, "http://ca.example.com/stale.der")
        .unwrap()
        .is_none());
    assert!(get_crl(&store, "http://ca.example.com/fresh.der")
        .unwrap()
        .is_some());
    assert!(get_crl(&store, "http://ca.example.com/no-expiry.der")
        .unwrap()
        .is_some());
}

#[test]
fn prune_noop_when_nothing_stale() {
    let store = test_store();
    let row = CrlCacheRow {
        crl_url: "http://ca.example.com/fresh.der".to_string(),
        crl_der: b"fresh".to_vec(),
        issuer_dn: None,
        next_update: Some("1900000000".to_string()),
        fetched_at: "1770000000".to_string(),
    };
    upsert_crl(&store, &row).unwrap();

    let deleted = prune_stale_crls(&store, "1800000000").unwrap();
    assert_eq!(deleted, 0);
}

// --- SP4c-3b: the CrlFetcher port + fetch-or-cache ---

use std::sync::atomic::{AtomicUsize, Ordering};

/// A fake fetcher that counts calls and returns a configured CRL (or fails).
/// No socket: the fetch-or-cache logic is proved offline.
#[derive(Debug)]
struct FakeFetcher {
    calls: AtomicUsize,
    der: Vec<u8>,
    next_update: Option<String>,
    fail: bool,
}

impl FakeFetcher {
    fn serving(next_update: Option<&str>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            der: b"fetched-crl-der".to_vec(),
            next_update: next_update.map(str::to_string),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::serving(None)
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl CrlFetcher for FakeFetcher {
    async fn fetch(&self, _crl_url: &str) -> Result<FetchedCrl, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err("fetch failed".to_string());
        }
        Ok(FetchedCrl {
            crl_der: self.der.clone(),
            issuer_dn: Some("CN=Fake CA".to_string()),
            next_update: self.next_update.clone(),
        })
    }
}

const URL: &str = "http://ca.example.com/crl.der";
const NOW: &str = "1800000000";

fn seed(store: &CryptoStore, der: &[u8], next_update: Option<&str>) {
    upsert_crl(
        store,
        &CrlCacheRow {
            crl_url: URL.to_string(),
            crl_der: der.to_vec(),
            issuer_dn: None,
            next_update: next_update.map(str::to_string),
            fetched_at: "1770000000".to_string(),
        },
    )
    .unwrap();
}

#[tokio::test]
async fn a_fresh_cache_is_used_without_a_fetch() {
    let store = test_store();
    seed(&store, b"cached-der", Some("1900000000")); // next_update > now
    let fetcher = FakeFetcher::serving(Some("1900000000"));

    fetch_or_cache_crl(&store, &fetcher, URL, NOW).await;

    assert_eq!(fetcher.calls(), 0, "a fresh cache must not trigger a fetch");
    let row = get_crl(&store, URL).unwrap().expect("row present");
    assert_eq!(row.crl_der, b"cached-der", "the cached CRL is untouched");
}

#[tokio::test]
async fn a_cache_with_no_next_update_is_kept_without_a_fetch() {
    let store = test_store();
    seed(&store, b"cached-der", None); // no known expiry → kept (prune policy)
    let fetcher = FakeFetcher::serving(Some("1900000000"));

    fetch_or_cache_crl(&store, &fetcher, URL, NOW).await;

    assert_eq!(fetcher.calls(), 0);
    assert_eq!(
        get_crl(&store, URL).unwrap().unwrap().crl_der,
        b"cached-der"
    );
}

#[tokio::test]
async fn a_stale_cache_triggers_a_fetch_and_upsert() {
    let store = test_store();
    seed(&store, b"stale-der", Some("1700000000")); // next_update < now
    let fetcher = FakeFetcher::serving(Some("1900000000"));

    fetch_or_cache_crl(&store, &fetcher, URL, NOW).await;

    assert_eq!(fetcher.calls(), 1, "a stale cache must trigger a fetch");
    let row = get_crl(&store, URL).unwrap().expect("row present");
    assert_eq!(row.crl_der, b"fetched-crl-der", "the fresh CRL replaces it");
    assert_eq!(row.next_update.as_deref(), Some("1900000000"));
    assert_eq!(row.fetched_at, NOW);
}

#[tokio::test]
async fn a_missing_cache_triggers_a_fetch_and_upsert() {
    let store = test_store();
    let fetcher = FakeFetcher::serving(Some("1900000000"));

    fetch_or_cache_crl(&store, &fetcher, URL, NOW).await;

    assert_eq!(fetcher.calls(), 1);
    let row = get_crl(&store, URL)
        .unwrap()
        .expect("the fetch upserts a row");
    assert_eq!(row.crl_der, b"fetched-crl-der");
    assert_eq!(row.issuer_dn.as_deref(), Some("CN=Fake CA"));
}

#[tokio::test]
async fn a_fetch_failure_degrades_and_never_errors() {
    let store = test_store();
    let fetcher = FakeFetcher::failing();

    // Must not panic and must not error: the open is never blocked.
    fetch_or_cache_crl(&store, &fetcher, URL, NOW).await;

    assert_eq!(fetcher.calls(), 1);
    assert!(
        get_crl(&store, URL).unwrap().is_none(),
        "a failed fetch caches nothing; revocation degrades to unchecked"
    );
}

#[tokio::test]
async fn a_fetch_failure_leaves_a_stale_cached_crl_in_place() {
    let store = test_store();
    seed(&store, b"stale-der", Some("1700000000"));
    let fetcher = FakeFetcher::failing();

    fetch_or_cache_crl(&store, &fetcher, URL, NOW).await;

    // The stale row stands (the chain still checks it as-of signing time);
    // the failed refetch neither removes it nor errors.
    let row = get_crl(&store, URL).unwrap().expect("stale row stands");
    assert_eq!(row.crl_der, b"stale-der");
}

#[tokio::test]
async fn the_noop_fetcher_never_caches() {
    let store = test_store();
    fetch_or_cache_crl(&store, &NoopCrlFetcher, URL, NOW).await;
    assert!(get_crl(&store, URL).unwrap().is_none());
}
