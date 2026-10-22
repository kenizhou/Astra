// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the crypto outbox: enqueue/drain states/requeue/settle.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    CryptoStore::open_in_memory().expect("open in-memory store")
}

#[test]
fn enqueue_then_list_round_trips_all_fields() {
    let store = test_store();
    let id = enqueue_crypto_send(
        &store,
        "a1",
        "smime",
        "secret plans",
        b"wire-bytes",
        b"filed-bytes",
        "me@x.com",
        &["peer@x.com".to_string()],
    )
    .unwrap();
    let rows = list_crypto_outbox(&store).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, id);
    assert_eq!(row.account_id, "a1");
    assert_eq!(row.standard, "smime");
    assert_eq!(row.subject, "secret plans");
    assert_eq!(row.wire_bytes, b"wire-bytes");
    assert_eq!(row.filed_bytes, b"filed-bytes");
    assert_eq!(row.state, "queued");
    assert!(!row.created_at.is_empty());
    assert!(row.error.is_none());
}

#[test]
fn due_lists_queued_and_stale_sending_but_never_unconfirmed_or_failed() {
    let store = test_store();
    let q = enqueue_crypto_send(&store, "a1", "smime", "q", b"w", b"f", "me@x.com", &[]).unwrap();
    let s = enqueue_crypto_send(&store, "a1", "smime", "s", b"w", b"f", "me@x.com", &[]).unwrap();
    let u = enqueue_crypto_send(&store, "a1", "smime", "u", b"w", b"f", "me@x.com", &[]).unwrap();
    let f = enqueue_crypto_send(&store, "a1", "smime", "f", b"w", b"f", "me@x.com", &[]).unwrap();
    mark_crypto_sending(&store, s).unwrap();
    mark_crypto_unconfirmed(&store, u, "mid-data loss").unwrap();
    mark_crypto_failed(&store, f, "550 refused").unwrap();

    let due: Vec<i64> = list_due_crypto_sends(&store)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    assert!(due.contains(&q), "queued drains: {due:?}");
    assert!(
        due.contains(&s),
        "a stale sending marker retries (no proof of delivery): {due:?}"
    );
    assert!(
        !due.contains(&u),
        "unconfirmed NEVER drains on its own: {due:?}"
    );
    assert!(!due.contains(&f), "failed waits for send-now: {due:?}");

    // The unconfirmed row keeps its cause for the row's display.
    let row = get_crypto_outbox_row(&store, u).unwrap().unwrap();
    assert_eq!(row.state, "unconfirmed");
    assert_eq!(row.error.as_deref(), Some("mid-data loss"));
}

#[test]
fn requeue_and_settle_complete_the_cycle() {
    let store = test_store();
    let id =
        enqueue_crypto_send(&store, "a1", "openpgp", "s", b"w", b"f", "me@x.com", &[]).unwrap();
    mark_crypto_failed(&store, id, "550 refused").unwrap();
    // Send-now: back to queued, the error kept for context.
    requeue_crypto_send(&store, id, Some("550 refused")).unwrap();
    let row = get_crypto_outbox_row(&store, id).unwrap().unwrap();
    assert_eq!(row.state, "queued");
    assert!(list_due_crypto_sends(&store)
        .unwrap()
        .iter()
        .any(|r| r.id == id));

    // Settled: the row leaves (delivered + filed, confirmed, or withdrawn).
    settle_crypto_send(&store, id).unwrap();
    assert!(list_crypto_outbox(&store).unwrap().is_empty());
    // Settling a missing row is a no-op, not an error.
    settle_crypto_send(&store, id).unwrap();
}

#[test]
fn sending_stamps_the_attempt_time() {
    let store = test_store();
    let id = enqueue_crypto_send(&store, "a1", "smime", "s", b"w", b"f", "me@x.com", &[]).unwrap();
    assert!(get_crypto_outbox_row(&store, id)
        .unwrap()
        .unwrap()
        .attempted_at
        .is_none());
    mark_crypto_sending(&store, id).unwrap();
    let row = get_crypto_outbox_row(&store, id).unwrap().unwrap();
    assert!(row.attempted_at.is_some());
}

#[test]
fn the_standard_and_state_checks_hold() {
    let store = test_store();
    let bad = store.conn().execute(
        "INSERT INTO crypto_outbox
         (account_id, standard, subject, wire_bytes, filed_bytes, state, created_at)
         VALUES ('a1', 'carrier-pigeon', 's', x'01', x'02', 'queued', '0')",
        [],
    );
    assert!(bad.is_err(), "the standard CHECK refuses nonsense");
    let bad_state = store.conn().execute(
        "INSERT INTO crypto_outbox
         (account_id, standard, subject, wire_bytes, filed_bytes, state, created_at)
         VALUES ('a1', 'smime', 's', x'01', x'02', 'limbo', '0')",
        [],
    );
    assert!(bad_state.is_err(), "the state CHECK refuses limbo");
}
