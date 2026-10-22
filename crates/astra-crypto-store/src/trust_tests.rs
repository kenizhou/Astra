// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the trust-decision audit layer: append-only history, latest-wins,
//! per-email latest, full per-address history. Ported from kylins' tests.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
    store
}

/// SQLite's `strftime('%s','now')` has second resolution; kylins' equivalent
/// tests slept a full second between puts. Sleeping per test is too slow for
/// a suite, so where the ordering matters these tests rely on the `id DESC`
/// tie-break instead (same-second inserts break by insertion id, which the
/// ORDER BY already encodes).
#[test]
fn put_twice_returns_latest_and_is_append_only() {
    let store = test_store();
    put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "smime",
        "fp1",
        "unverified",
        None,
    )
    .unwrap();
    put_trust_decision(&store, "a1", "peer@x.com", "smime", "fp1", "verified", None).unwrap();

    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "smime", "fp1")
        .unwrap()
        .expect("a decision exists");
    assert_eq!(
        latest.decision, "verified",
        "same-second ties break by id DESC (the later insert wins)"
    );

    // Append-only: two puts for the same key must yield exactly two rows.
    let (count,): (i64,) = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM trust_decisions WHERE account_id = 'a1' AND peer_email = 'peer@x.com' AND standard = 'smime' AND fingerprint = 'fp1'",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();
    assert_eq!(count, 2, "trust_decisions must be append-only (COUNT == 2)");
}

#[test]
fn get_latest_returns_none_when_absent() {
    let store = test_store();
    let got = get_latest_trust_decision(&store, "a1", "nobody@x.com", "smime", "fpX").unwrap();
    assert!(got.is_none());
}

#[test]
fn put_carries_evidence_json() {
    let store = test_store();
    put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "smime",
        "fp1",
        "personal",
        Some(r#"{"source":"manual"}"#),
    )
    .unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "smime", "fp1")
        .unwrap()
        .unwrap();
    assert_eq!(latest.decision, "personal");
    assert_eq!(
        latest.evidence_json.as_deref(),
        Some(r#"{"source":"manual"}"#)
    );
}

/// `list_latest_decisions_for_key` returns ONE row per distinct
/// `peer_email`, the LATEST decision for it, across multiple emails recorded
/// against the same fingerprint. Append-only history is preserved
/// (superseded rows remain in the table, just not returned).
#[test]
fn list_latest_decisions_for_key_returns_latest_per_email() {
    let store = test_store();

    // alice: rejected, later overridden to verified (append-only).
    put_trust_decision(
        &store,
        "a1",
        "alice@x.com",
        "smime",
        "fp9",
        "rejected",
        None,
    )
    .unwrap();
    put_trust_decision(
        &store,
        "a1",
        "alice@x.com",
        "smime",
        "fp9",
        "verified",
        None,
    )
    .unwrap();
    // bob: single decision.
    put_trust_decision(
        &store,
        "a1",
        "bob@x.com",
        "smime",
        "fp9",
        "unverified",
        None,
    )
    .unwrap();
    // A DIFFERENT fingerprint's decision must NOT appear.
    put_trust_decision(
        &store,
        "a1",
        "carol@x.com",
        "smime",
        "fpOTHER",
        "verified",
        None,
    )
    .unwrap();
    // A different account's decision on the SAME fingerprint must NOT appear.
    put_trust_decision(&store, "a2", "dave@x.com", "smime", "fp9", "verified", None).unwrap();

    let rows = list_latest_decisions_for_key(&store, "a1", "smime", "fp9").unwrap();
    assert_eq!(rows.len(), 2, "one row per distinct peer_email: {rows:?}");
    // Sorted by peer_email for deterministic output.
    assert_eq!(rows[0].peer_email, "alice@x.com");
    assert_eq!(
        rows[0].decision, "verified",
        "latest decision wins over the superseded 'rejected'"
    );
    assert_eq!(rows[1].peer_email, "bob@x.com");
    assert_eq!(rows[1].decision, "unverified");

    // Append-only: the superseded alice row is still in the table.
    let (count,): (i64,) = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM trust_decisions WHERE account_id = 'a1' AND fingerprint = 'fp9'",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();
    assert_eq!(count, 3, "append-only history preserved (2 alice + 1 bob)");
}

/// An empty peer_email (the unknown-key dialog path) is a legitimate bucket:
/// returned as its own row.
#[test]
fn list_latest_decisions_for_key_includes_empty_peer_email() {
    let store = test_store();
    put_trust_decision(&store, "a1", "", "smime", "fpE", "verified", None).unwrap();
    let rows = list_latest_decisions_for_key(&store, "a1", "smime", "fpE").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].peer_email, "");
    assert_eq!(rows[0].decision, "verified");
}

/// `list_decisions_for_email` returns the FULL (non-deduped) history for one
/// address, latest first, scoped to `(account, standard, email)`: the
/// anti-spoof continuity check needs the superseded rows (a prior acceptance
/// of a DIFFERENT fingerprint is the signal it looks for).
#[test]
fn list_decisions_for_email_returns_full_history_latest_first() {
    let store = test_store();

    // Old key accepted, then a NEW key accepted for the same address (the
    // key-continuity change the guard must be able to see).
    put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "smime",
        "fpOLD",
        "verified",
        None,
    )
    .unwrap();
    put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "smime",
        "fpNEW",
        "unverified",
        None,
    )
    .unwrap();
    // Noise that must NOT appear: a different address, a different standard
    // for the same address, another account's row.
    put_trust_decision(
        &store,
        "a1",
        "other@x.com",
        "smime",
        "fpOLD",
        "verified",
        None,
    )
    .unwrap();
    put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "openpgp",
        "fpPGP",
        "verified",
        None,
    )
    .unwrap();
    put_trust_decision(
        &store,
        "a2",
        "peer@x.com",
        "smime",
        "fpOLD",
        "verified",
        None,
    )
    .unwrap();

    let rows = list_decisions_for_email(&store, "a1", "smime", "peer@x.com").unwrap();
    assert_eq!(rows.len(), 2, "both history rows, no noise: {rows:?}");
    assert_eq!(
        rows[0].fingerprint, "fpNEW",
        "latest first (id DESC tie-break)"
    );
    assert_eq!(rows[0].decision, "unverified");
    assert_eq!(rows[1].fingerprint, "fpOLD");
    assert_eq!(rows[1].decision, "verified");

    // An address with no history returns an empty Vec (not an error).
    let empty = list_decisions_for_email(&store, "a1", "smime", "nobody@x.com").unwrap();
    assert!(empty.is_empty());
}

#[test]
fn decision_check_constraint_rejects_unknown_values() {
    let store = test_store();
    let err = put_trust_decision(&store, "a1", "p@x.com", "smime", "fp1", "maybe", None);
    assert!(
        err.is_err(),
        "the CHECK constraint rejects unknown decisions"
    );
}
