// SPDX-License-Identifier: GPL-3.0-only
//! Knowledge store tests: the 007 migration lands the table, an entry
//! round trips every field, the account's list reads newest first and
//! stays per account, the exact-text dedup guard refuses a repeated
//! text loudly while another account keeps the same text as its own,
//! and forget removes the entry and reports an unknown id.

use crate::{AiStore, CorrelationId, KnowledgeId, NewKnowledgeEntry};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One entry with every field set to a distinct value so a round trip
/// cannot pass by coincidence. The kind rides as the caller's wire
/// word: the closed set is the extraction prompt's, pinned in
/// astra-app.
fn entry(id: &str, account: &str, text: &str) -> NewKnowledgeEntry {
    NewKnowledgeEntry {
        id: KnowledgeId::new(id.to_string()),
        account: account.to_string(),
        kind: "preference".to_string(),
        text: text.to_string(),
        source_message_account: account.to_string(),
        source_message_key: "m-17".to_string(),
        correlation: CorrelationId::new("corr-1".to_string()),
    }
}

#[test]
fn the_knowledge_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'knowledge'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "the knowledge table is missing after migrations");
}

#[test]
fn an_entry_round_trips_every_field() {
    let store = store();
    let row = store
        .insert_knowledge(&entry("k-1", "work", "Ada prefers plain text mail."))
        .expect("the entry inserts");
    assert_eq!(row.id.as_str(), "k-1");
    assert_eq!(row.account, "work");
    assert_eq!(row.kind, "preference");
    assert_eq!(row.text, "Ada prefers plain text mail.");
    assert_eq!(row.source_message_account, "work");
    assert_eq!(row.source_message_key, "m-17");
    assert_eq!(row.correlation.as_str(), "corr-1");
    assert!(row.created_at > 0, "the store stamps created_at itself");
    let listed = store.knowledge_for_account("work").expect("the list runs");
    assert_eq!(listed, vec![row], "the row read back matches the insert");
}

#[test]
fn the_account_list_reads_newest_first_and_stays_per_account() {
    let store = store();
    store
        .insert_knowledge(&entry("k-1", "work", "The launch date is the ninth."))
        .expect("the older entry inserts");
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        conn.execute(
            "UPDATE knowledge SET created_at = strftime('%s','now') - 3600 WHERE id = 'k-1'",
            [],
        )
        .expect("the older entry is backdated an hour");
    }
    store
        .insert_knowledge(&entry("k-2", "work", "Ada reviews on Fridays."))
        .expect("the newer entry inserts");
    store
        .insert_knowledge(&entry("k-3", "home", "The boiler service is due."))
        .expect("another account's entry inserts");
    let ids: Vec<String> = store
        .knowledge_for_account("work")
        .expect("the list runs")
        .into_iter()
        .map(|row| row.id.as_str().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["k-2".to_string(), "k-1".to_string()],
        "newest first, and another account's entries stay out",
    );
}

#[test]
fn the_exact_text_guard_refuses_a_repeated_text_loudly() {
    let store = store();
    store
        .insert_knowledge(&entry("k-1", "work", "Ada prefers plain text mail."))
        .expect("the first entry inserts");
    let err = store
        .insert_knowledge(&entry("k-2", "work", "Ada prefers plain text mail."))
        .expect_err("the same account must refuse the same text");
    let message = err.to_string();
    assert!(message.contains("ai store sql failure"), "{message}");
    assert!(message.contains("UNIQUE"), "{message}");
    assert_eq!(
        store
            .knowledge_for_account("work")
            .expect("the list runs")
            .len(),
        1,
        "the refused insert left the one entry",
    );
    store
        .insert_knowledge(&entry("k-3", "home", "Ada prefers plain text mail."))
        .expect("another account keeps the same text as its own");
    assert_eq!(
        store
            .knowledge_for_account("home")
            .expect("the list runs")
            .len(),
        1,
        "the guard keys on the account, not the text alone",
    );
}

#[test]
fn forget_removes_the_entry_and_reports_an_unknown_id() {
    let store = store();
    store
        .insert_knowledge(&entry("k-1", "work", "Ada reviews on Fridays."))
        .expect("the entry inserts");
    assert!(
        store
            .forget_knowledge(&KnowledgeId::new("k-1".to_string()))
            .expect("the forget runs"),
        "the known entry reports forgotten",
    );
    assert!(
        store
            .knowledge_for_account("work")
            .expect("the list runs")
            .is_empty(),
        "the entry is gone",
    );
    assert!(
        !store
            .forget_knowledge(&KnowledgeId::new("k-1".to_string()))
            .expect("the forget runs"),
        "a second forget changes nothing and says so",
    );
}

#[test]
fn generated_knowledge_ids_are_distinct_and_nonempty() {
    let first = KnowledgeId::generate();
    let second = KnowledgeId::generate();
    assert!(!first.as_str().is_empty());
    assert_ne!(first, second);
}
