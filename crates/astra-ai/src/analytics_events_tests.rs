// SPDX-License-Identifier: GPL-3.0-only
//! Analytics events ledger tests: the table's migration, the round trip
//! with every field intact, the store-stamped creation time, the
//! newest-first read, and the never-content pin at the column-shape
//! level.

use std::collections::BTreeMap;

use crate::{AiStore, NewAnalyticsEvent};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One event to record: every field set to a distinct value so a round
/// trip cannot pass by coincidence.
fn event(shape: &str) -> NewAnalyticsEvent {
    NewAnalyticsEvent {
        shape: shape.to_owned(),
        counts: BTreeMap::from([("messages".to_owned(), 12), ("accounts".to_owned(), 2)]),
    }
}

#[test]
fn migrations_create_the_analytics_events_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'analytics_events'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "analytics_events missing after migrations");
}

#[test]
fn analytics_event_round_trips_every_field() {
    let store = store();
    let landed = store
        .record_analytics_event(&event("sync_pass"))
        .expect("the event records");

    assert_eq!(landed.shape, "sync_pass");
    assert_eq!(
        landed.counts,
        BTreeMap::from([("messages".to_owned(), 12), ("accounts".to_owned(), 2),]),
        "the named counts round trip value for value"
    );

    let read = store.recent_analytics_events(10).expect("the read runs");
    assert_eq!(read.len(), 1, "one row lands per event");
    assert_eq!(read[0].shape, "sync_pass");
    assert_eq!(read[0].counts, landed.counts);
}

#[test]
fn an_event_with_no_counts_round_trips_as_the_empty_object() {
    let store = store();
    let landed = store
        .record_analytics_event(&NewAnalyticsEvent {
            shape: "app_launch".to_owned(),
            counts: BTreeMap::new(),
        })
        .expect("the event records");
    assert!(landed.counts.is_empty());
    assert_eq!(
        store
            .recent_analytics_events(10)
            .expect("the read runs")
            .len(),
        1
    );
}

#[test]
fn created_at_is_stamped_by_the_store() {
    let store = store();
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock runs forward")
        .as_secs()
        .cast_signed();
    let landed = store
        .record_analytics_event(&event("app_launch"))
        .expect("the event records");
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock runs forward")
        .as_secs()
        .cast_signed();
    assert!(
        landed.created_at >= before && landed.created_at <= after,
        "the store stamps created_at itself: {} outside [{before}, {after}]",
        landed.created_at,
    );
}

#[test]
fn recent_analytics_events_orders_newest_first() {
    let store = store();
    store
        .record_analytics_event(&event("app_launch"))
        .expect("the first records");
    store
        .record_analytics_event(&event("sync_pass"))
        .expect("the second records");

    let read = store.recent_analytics_events(10).expect("the read runs");
    assert_eq!(
        read.iter()
            .map(|row| row.shape.as_str())
            .collect::<Vec<_>>(),
        ["sync_pass", "app_launch"],
        "newest first, the insertion order reversed"
    );
    let limited = store
        .recent_analytics_events(1)
        .expect("the limited read runs");
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].shape, "sync_pass");
}

/// The never-content pin at the column level: the table holds the shape
/// name, the counted column, and the stamps, and nothing that could carry
/// a query, an address, a subject, or a body. A column that does not
/// exist cannot be filled by a later change that forgets the rule.
#[test]
fn the_table_shape_holds_no_content_column() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let mut statement = conn
        .prepare("PRAGMA table_info(analytics_events)")
        .expect("the table info reads");
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("the columns read")
        .collect::<Result<Vec<_>, _>>()
        .expect("the column names parse");
    let mut sorted = columns.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        ["counts", "created_at", "id", "shape"],
        "the analytics_events columns are a shape name and counts only"
    );

    // And the counted column itself parses as nothing but integers: a
    // row the ledger holds cannot name a subject or an address in a
    // count's value.
    drop(statement);
    drop(conn);
    store
        .record_analytics_event(&event("sync_pass"))
        .expect("the event records");
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let counts: String = conn
        .query_row("SELECT counts FROM analytics_events", [], |row| row.get(0))
        .expect("the counts column reads");
    let parsed: BTreeMap<String, i64> = serde_json::from_str(&counts).expect("the counts parse");
    assert!(
        parsed.values().all(|value| *value >= 0),
        "every count is an integer: {counts}"
    );
}
