// SPDX-License-Identifier: GPL-3.0-only
// Chips-dismissed store tests: the 0006 migration lands the marker table,
// a dismissal reads back as present, dismissing twice stays one row, and
// each message keeps its own marker.

use crate::AiStore;

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

#[test]
fn the_chip_dismissals_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'chips_dismissed'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(
        found, 1,
        "the chips_dismissed table is missing after migrations"
    );
}

#[test]
fn a_dismissal_reads_back_as_present() {
    let store = store();
    assert!(
        !store.chips_dismissed("work", "m1").expect("the read runs"),
        "a message never dismissed answers absent"
    );
    store
        .dismiss_chips("work", "m1")
        .expect("the dismissal lands");
    assert!(
        store.chips_dismissed("work", "m1").expect("the read runs"),
        "the dismissed message answers present"
    );
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let stamped: i64 = conn
        .query_row(
            "SELECT created_at FROM chips_dismissed
             WHERE message_account = 'work' AND message_key = 'm1'",
            [],
            |row| row.get(0),
        )
        .expect("the marker row reads");
    assert!(stamped > 0, "the store stamps the dismissal itself");
}

#[test]
fn dismissing_twice_stays_one_marker_row() {
    let store = store();
    store
        .dismiss_chips("work", "m1")
        .expect("the first dismissal lands");
    store
        .dismiss_chips("work", "m1")
        .expect("the repeat dismissal is idempotent");
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chips_dismissed
             WHERE message_account = 'work' AND message_key = 'm1'",
            [],
            |row| row.get(0),
        )
        .expect("the count reads");
    assert_eq!(rows, 1, "one marker per message, never a row per click");
}

#[test]
fn each_message_keeps_its_own_marker() {
    let store = store();
    store
        .dismiss_chips("work", "m1")
        .expect("the dismissal lands");
    assert!(
        !store.chips_dismissed("work", "m2").expect("runs"),
        "another message of the account stays undismissed",
    );
    assert!(
        !store.chips_dismissed("home", "m1").expect("runs"),
        "another account's message stays undismissed",
    );
}
