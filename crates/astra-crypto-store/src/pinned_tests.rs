// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the pinned-key layer. Ported from kylins' pinned_keys tests.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    CryptoStore::open_in_memory().expect("open in-memory store")
}

#[test]
fn set_then_list_round_trips() {
    let store = test_store();
    set_pinned_key(&store, "peer@x.com", "openpgp", "fpPGP").unwrap();
    let pins = list_pinned_keys(&store, "peer@x.com").unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].standard, "openpgp");
    assert_eq!(pins[0].fingerprint, "fpPGP");
}

#[test]
fn emails_are_normalized_trimmed_and_lowercased() {
    let store = test_store();
    set_pinned_key(&store, "  Peer@X.COM ", "openpgp", "fpA").unwrap();
    let pins = list_pinned_keys(&store, "peer@x.com").unwrap();
    assert_eq!(pins.len(), 1, "the row keys the normalized email");
    let (rows,): (i64,) = store
        .conn()
        .query_row("SELECT COUNT(*) FROM contact_pinned_keys", [], |r| {
            Ok((r.get(0)?,))
        })
        .unwrap();
    assert_eq!(rows, 1, "one row per email, not per spelling");
}

#[test]
fn set_replaces_same_standard_pin_but_preserves_other_standards() {
    let store = test_store();
    set_pinned_key(&store, "peer@x.com", "openpgp", "fpOLD").unwrap();
    set_pinned_key(&store, "peer@x.com", "smime", "fpSMIME").unwrap();
    // Re-pinning openpgp replaces ONLY the openpgp entry.
    set_pinned_key(&store, "peer@x.com", "openpgp", "fpNEW").unwrap();

    let mut pins = list_pinned_keys(&store, "peer@x.com").unwrap();
    pins.sort_by(|a, b| a.standard.cmp(&b.standard));
    assert_eq!(pins.len(), 2, "one pin per standard: {pins:?}");
    assert_eq!(pins[0].standard, "openpgp");
    assert_eq!(pins[0].fingerprint, "fpNEW");
    assert_eq!(pins[1].standard, "smime");
    assert_eq!(pins[1].fingerprint, "fpSMIME");
}

#[test]
fn remove_deletes_only_the_matching_pin_and_is_idempotent() {
    let store = test_store();
    set_pinned_key(&store, "peer@x.com", "openpgp", "fpPGP").unwrap();
    set_pinned_key(&store, "peer@x.com", "smime", "fpSMIME").unwrap();

    remove_pinned_key(&store, "peer@x.com", "openpgp", "fpPGP").unwrap();
    let pins = list_pinned_keys(&store, "peer@x.com").unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].standard, "smime");

    // Second remove of the same pin is a no-op (idempotent).
    remove_pinned_key(&store, "peer@x.com", "openpgp", "fpPGP").unwrap();
    // Remove on a row-less email is a no-op, not an error.
    remove_pinned_key(&store, "ghost@x.com", "openpgp", "fpX").unwrap();
}

#[test]
fn list_tolerates_malformed_blobs_and_legacy_data_member() {
    let store = test_store();
    // Legacy shape documented by the original migration comment: an extra
    // `data` member plus one malformed entry (missing fingerprint); the
    // valid entry must survive, the malformed one is skipped.
    store
        .conn()
        .execute(
            "INSERT INTO contact_pinned_keys (email, pinned_keys_json) VALUES (?1, ?2)",
            rusqlite::params![
                "legacy@x.com",
                r#"[{"standard":"openpgp","fingerprint":"fpA","data":"AAAA"},{"standard":"smime"}]"#
            ],
        )
        .unwrap();
    let pins = list_pinned_keys(&store, "legacy@x.com").unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].fingerprint, "fpA");

    // A totally malformed blob parses to an empty list, never an error.
    store
        .conn()
        .execute(
            "INSERT INTO contact_pinned_keys (email, pinned_keys_json) VALUES (?1, ?2)",
            rusqlite::params!["broken@x.com", "not json"],
        )
        .unwrap();
    let pins = list_pinned_keys(&store, "broken@x.com").unwrap();
    assert!(pins.is_empty());
}

#[test]
fn set_rejects_empty_standard_or_fingerprint() {
    let store = test_store();
    assert!(set_pinned_key(&store, "p@x.com", "", "fp").is_err());
    assert!(set_pinned_key(&store, "p@x.com", "openpgp", "  ").is_err());
}
