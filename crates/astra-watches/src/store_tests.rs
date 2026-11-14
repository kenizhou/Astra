// SPDX-License-Identifier: GPL-3.0-only
//! Store tests: the migration applies and claims once, a watch row round
//! trips every field, a re-watch is idempotent, the unwatch (the one-shot
//! consume) takes the row and reports it, the listing reads rowid order
//! across accounts, the unwatch is account-scoped, and a malformed row is
//! skipped rather than fatal.

use engine_api::{AccountId, ProviderKey};

use crate::{NewWatch, WatchStore};

/// A migrated in-memory store.
fn store() -> WatchStore {
    WatchStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// A fresh scratch directory of its own, so a recycled pid never revisits a
/// stale store from an earlier run.
fn scratch(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is past the epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("astra-watches-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A full watch row: every field set to a distinct value so a round trip
/// cannot pass by coincidence.
fn new_watch(account: &str, thread: &str, member: &str) -> NewWatch {
    NewWatch {
        account: AccountId::try_from(account).unwrap(),
        thread_id: thread.to_owned(),
        member_key: ProviderKey::new(member).unwrap(),
    }
}

#[test]
fn migrations_create_the_watch_replies_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    for table in ["watch_replies", "watch_migrations"] {
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
fn a_watch_round_trips_every_field() {
    let store = store();
    let inserted = store
        .watch(&new_watch("work", "th-1", "m1"))
        .expect("the watch records");
    assert!(inserted, "a fresh watch answers inserted");

    let listed = store.list().expect("the listing reads");
    assert_eq!(listed.len(), 1, "one row landed");
    let entry = &listed[0];
    assert_eq!(entry.account.as_str(), "work");
    assert_eq!(entry.thread_id, "th-1");
    assert_eq!(entry.member_key.as_str(), "m1");
    assert!(entry.created_at > 0, "the store stamps created_at");
}

#[test]
fn a_re_watch_is_idempotent_and_keeps_the_standing_row() {
    let store = store();
    assert!(store.watch(&new_watch("work", "th-1", "m1")).unwrap());
    // A re-watch on the same (account, thread) inserts nothing and keeps the
    // original member link: the row is a membership mark, not a re-set.
    assert!(
        !store.watch(&new_watch("work", "th-1", "m2")).unwrap(),
        "a re-watch answers not-inserted"
    );
    let listed = store.list().expect("the listing reads");
    assert_eq!(listed.len(), 1, "no second row stacks");
    assert_eq!(
        listed[0].member_key.as_str(),
        "m1",
        "the standing member link survives the re-watch"
    );
}

#[test]
fn the_unwatch_takes_the_row_and_reports_it() {
    let store = store();
    store.watch(&new_watch("work", "th-1", "m1")).unwrap();
    let account = AccountId::try_from("work").unwrap();

    assert!(
        store.unwatch(&account, "th-1").expect("the unwatch runs"),
        "the standing row is removed"
    );
    assert!(
        !store
            .unwatch(&account, "th-1")
            .expect("the second unwatch runs"),
        "the one-shot consume is idempotent: a second clear removes nothing"
    );
    assert!(
        store.list().expect("the listing reads").is_empty(),
        "the row is gone from the listing"
    );
}

#[test]
fn the_listing_reads_rowid_order_across_accounts() {
    let store = store();
    store.watch(&new_watch("work", "th-2", "m2")).unwrap();
    store.watch(&new_watch("home", "th-1", "m1")).unwrap();
    store.watch(&new_watch("work", "th-3", "m3")).unwrap();

    let listed = store.list().expect("the listing reads");
    let handles: Vec<(&str, &str)> = listed
        .iter()
        .map(|entry| (entry.account.as_str(), entry.thread_id.as_str()))
        .collect();
    assert_eq!(
        handles,
        [("work", "th-2"), ("home", "th-1"), ("work", "th-3")],
        "cross-account, insertion (rowid) order"
    );
}

#[test]
fn the_unwatch_is_scoped_to_the_account() {
    let store = store();
    store.watch(&new_watch("work", "th-1", "m1")).unwrap();
    store.watch(&new_watch("home", "th-1", "m1")).unwrap();
    let home = AccountId::try_from("home").unwrap();

    // The same thread id in another account is a different row: clearing it
    // leaves the work account's watch standing.
    assert!(store.unwatch(&home, "th-1").unwrap());
    let listed = store.list().expect("the listing reads");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].account.as_str(), "work");
}

#[test]
fn a_malformed_row_is_skipped_not_fatal() {
    let store = store();
    store.watch(&new_watch("work", "good", "m1")).unwrap();
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        // An empty account id is a row whose identity cannot parse.
        conn.execute(
            "INSERT INTO watch_replies (account_id, thread_id, member_key, created_at)
             VALUES ('', 'bad', 'm9', strftime('%s','now'))",
            [],
        )
        .expect("the malformed row inserts");
    }
    let listed = store.list().expect("the listing tolerates the bad row");
    assert_eq!(listed.len(), 1, "the malformed row is skipped");
    assert_eq!(listed[0].thread_id, "good");
}

/// The bookkeeping rows of one store, in order.
fn versions(store: &WatchStore) -> Vec<i64> {
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let mut statement = conn
        .prepare("SELECT version FROM watch_migrations ORDER BY version")
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
    let path = dir.join("watches.sqlite3");
    {
        let store = WatchStore::open(&path).expect("the first open migrates");
        assert_eq!(versions(&store), vec![1], "one row per applied migration");
    }
    let reopened = WatchStore::open(&path).expect("the reopen claims nothing new");
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
    let path = dir.join("watches.sqlite3");
    std::thread::scope(|scope| {
        let openers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| WatchStore::open(&path)))
            .collect();
        for opener in openers {
            opener
                .join()
                .expect("the opener thread ends")
                .expect("a concurrent first open never fails on the bookkeeping row");
        }
    });
    let store = WatchStore::open(&path).expect("the store opens after the race");
    assert_eq!(
        versions(&store),
        vec![1],
        "the race applied each migration once"
    );
    std::fs::remove_dir_all(&dir).ok();
}
