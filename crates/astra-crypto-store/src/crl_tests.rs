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
