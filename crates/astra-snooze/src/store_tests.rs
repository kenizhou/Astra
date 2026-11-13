// SPDX-License-Identifier: GPL-3.0-only
//! Store tests: the migration applies and claims once, a snooze row round
//! trips every field, the due query reads the `wake_at <= now` boundary, a
//! removal takes the row, and a malformed row is skipped rather than fatal.

use engine_api::{AccountId, MessageIdHeader, ProviderKey};

use crate::{NewSnooze, SnoozeStore};

/// A migrated in-memory store.
fn store() -> SnoozeStore {
    SnoozeStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// A fresh scratch directory of its own, so a recycled pid never revisits a
/// stale store from an earlier run.
fn scratch(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is past the epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("astra-snooze-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A full snooze row: every field set to a distinct value so a round trip
/// cannot pass by coincidence.
fn new_snooze(message_id: &str, wake_at: i64) -> NewSnooze {
    NewSnooze {
        account: AccountId::try_from("work").unwrap(),
        message_id: MessageIdHeader::new(message_id).unwrap(),
        provider_key: ProviderKey::new("m1").unwrap(),
        wake_at,
        origin_folder: "inbox".to_owned(),
        subject: "Quarterly report".to_owned(),
        sender: "ada@example.test".to_owned(),
    }
}

#[test]
fn migrations_create_the_snoozes_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    for table in ["snoozes", "snooze_migrations"] {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap_or_else(|e| panic!("sqlite_master read: {e}"));
        assert_eq!(found, 1, "{table} missing after migrations");
    }
}

#[test]
fn a_snooze_round_trips_every_field() {
    let store = store();
    let landed = store
        .record(&new_snooze("msg-1@example.test", 1_000))
        .expect("the snooze records");
    assert_eq!(landed.account.as_str(), "work");
    assert_eq!(landed.message_id.as_str(), "msg-1@example.test");
    assert_eq!(landed.provider_key.as_str(), "m1");
    assert_eq!(landed.wake_at, 1_000);
    assert_eq!(landed.origin_folder, "inbox");
    assert_eq!(landed.subject, "Quarterly report");
    assert_eq!(landed.sender, "ada@example.test");
    assert!(landed.created_at > 0, "the store stamps created_at");

    let listed = store.list().expect("the listing reads");
    assert_eq!(listed, vec![landed], "the row reads back whole");
}

#[test]
fn the_due_query_reads_the_wake_at_boundary() {
    let store = store();
    store
        .record(&new_snooze("early@example.test", 1_000))
        .expect("early records");
    store
        .record(&new_snooze("boundary@example.test", 2_000))
        .expect("boundary records");
    store
        .record(&new_snooze("late@example.test", 3_000))
        .expect("late records");

    // wake_at <= now: the boundary row itself is due, the later one is not.
    let due = store.due(2_000).expect("the due query runs");
    let ids: Vec<&str> = due.iter().map(|e| e.message_id.as_str()).collect();
    assert_eq!(
        ids,
        ["early@example.test", "boundary@example.test"],
        "the boundary instant is due, the future one is not, earliest first"
    );
    assert!(
        store.due(999).expect("the due query runs").is_empty(),
        "nothing is due before the earliest wake"
    );
}

#[test]
fn a_removal_takes_the_row() {
    let store = store();
    let landed = store
        .record(&new_snooze("msg-1@example.test", 1_000))
        .expect("the snooze records");
    store.remove(landed.id).expect("the removal runs");
    assert!(
        store.list().expect("the listing reads").is_empty(),
        "the removal takes the row from the listing"
    );
    assert!(
        store.due(2_000).expect("the due query runs").is_empty(),
        "the removal takes the row from the due query"
    );
}

#[test]
fn a_malformed_row_is_skipped_not_fatal() {
    let store = store();
    store
        .record(&new_snooze("good@example.test", 1_000))
        .expect("the good row records");
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        // An empty account id is a row whose identity cannot parse.
        conn.execute(
            "INSERT INTO snoozes (
                 created_at, account_id, message_id, provider_key, wake_at,
                 origin_folder, subject, sender
             ) VALUES (strftime('%s','now'), '', 'bad@example.test', 'm9', 1_000,
                 'inbox', 'Bad', 'bob@example.test')",
            [],
        )
        .expect("the malformed row inserts");
    }
    let due = store
        .due(2_000)
        .expect("the due query tolerates the bad row");
    let ids: Vec<&str> = due.iter().map(|e| e.message_id.as_str()).collect();
    assert_eq!(ids, ["good@example.test"], "the malformed row is skipped");
    assert_eq!(store.list().expect("the listing reads").len(), 1);
}

/// The bookkeeping rows of one store, in order.
fn versions(store: &SnoozeStore) -> Vec<i64> {
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let mut statement = conn
        .prepare("SELECT version FROM snooze_migrations ORDER BY version")
        .expect("the bookkeeping query prepares");
    statement
        .query_map([], |row| row.get(0))
        .expect("the bookkeeping query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("the bookkeeping rows read")
}

#[test]
fn the_bookkeeping_row_lands_once_and_survives_the_reopen() {
    let dir = scratch("store");
    let path = dir.join("snooze.sqlite3");
    {
        let store = SnoozeStore::open(&path).expect("the first open migrates");
        assert_eq!(versions(&store), vec![1], "one row per applied migration");
    }
    let reopened = SnoozeStore::open(&path).expect("the reopen claims nothing new");
    assert_eq!(
        versions(&reopened),
        vec![1],
        "the reopen records no second bookkeeping row"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn concurrent_first_opens_all_succeed_with_one_bookkeeping_row() {
    let dir = scratch("race");
    let path = dir.join("snooze.sqlite3");
    std::thread::scope(|scope| {
        let openers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| SnoozeStore::open(&path)))
            .collect();
        for opener in openers {
            opener
                .join()
                .expect("the opener thread ends")
                .expect("a concurrent first open never fails on the bookkeeping row");
        }
    });
    let store = SnoozeStore::open(&path).expect("the store opens after the race");
    assert_eq!(
        versions(&store),
        vec![1],
        "the race applied each migration once"
    );
    std::fs::remove_dir_all(&dir).ok();
}
