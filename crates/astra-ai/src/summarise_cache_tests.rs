// SPDX-License-Identifier: GPL-3.0-only
//! Summarise cache store tests: the 005 migration lands the table, a put
//! hits while the stored body hash matches the asked one, a changed hash
//! misses, a re-put under the new hash invalidates the old entry, and
//! each message keeps its own row.

use crate::AiStore;

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

#[test]
fn the_summarise_cache_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'summarise_cache'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(
        found, 1,
        "the summarise_cache table is missing after migrations"
    );
}

#[test]
fn a_put_hits_only_while_the_hash_matches() {
    let store = store();
    assert!(
        store
            .cached_summary("work", "m1", "hash-a")
            .expect("the read runs")
            .is_none(),
        "a message never summarised answers none"
    );
    store
        .cache_summary("work", "m1", "hash-a", "Ada asks for the launch date.")
        .expect("the summary caches");
    assert_eq!(
        store
            .cached_summary("work", "m1", "hash-a")
            .expect("the hit read runs"),
        Some("Ada asks for the launch date.".to_owned()),
        "the matching hash hits without a call",
    );
    assert!(
        store
            .cached_summary("work", "m1", "hash-b")
            .expect("the miss read runs")
            .is_none(),
        "a different body hash misses",
    );
}

#[test]
fn a_re_put_under_a_new_hash_replaces_the_entry() {
    let store = store();
    store
        .cache_summary("work", "m1", "hash-a", "The first summary.")
        .expect("the first summary caches");
    store
        .cache_summary("work", "m1", "hash-b", "The second summary.")
        .expect("the re-put under the new hash stores");
    assert!(
        store
            .cached_summary("work", "m1", "hash-a")
            .expect("runs")
            .is_none(),
        "the old hash no longer hits",
    );
    assert_eq!(
        store.cached_summary("work", "m1", "hash-b").expect("runs"),
        Some("The second summary.".to_owned()),
        "one row per message, the newest hash's summary",
    );
}

#[test]
fn each_message_keeps_its_own_entry() {
    let store = store();
    for (key, summary) in [("m1", "One."), ("m2", "Two.")] {
        store
            .cache_summary("work", key, "hash-a", summary)
            .expect("the summary caches");
    }
    assert_eq!(
        store.cached_summary("work", "m1", "hash-a").expect("runs"),
        Some("One.".to_owned()),
    );
    assert_eq!(
        store.cached_summary("work", "m2", "hash-a").expect("runs"),
        Some("Two.".to_owned()),
    );
    assert!(
        store
            .cached_summary("home", "m1", "hash-a")
            .expect("runs")
            .is_none(),
        "another account's cache stays its own row",
    );
}
