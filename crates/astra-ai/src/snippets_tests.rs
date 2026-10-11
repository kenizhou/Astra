// SPDX-License-Identifier: GPL-3.0-only
//! Snippet store tests: the 008 migration lands the table, a snippet
//! round trips every field, the list reads the most recently touched
//! first, an update rewrites the fields and touches updated_at, and
//! delete removes the snippet and reports an unknown id.

use crate::{AiStore, NewSnippet, SnippetId};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One snippet with every field set to a distinct value so a round trip
/// cannot pass by coincidence.
fn snippet(id: &str, title: &str) -> NewSnippet {
    NewSnippet {
        id: SnippetId::new(id.to_string()),
        title: title.to_string(),
        body: format!("The {title} body."),
    }
}

/// Backdates one snippet's stamps by an hour, so ordering and touch
/// assertions do not depend on the same-second insert stamps.
fn backdate_stamps(store: &AiStore, id: &str) {
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    conn.execute(
        "UPDATE snippets SET
             created_at = strftime('%s','now') - 3600,
             updated_at = strftime('%s','now') - 3600
         WHERE id = ?1",
        [id],
    )
    .expect("the stamps backdate");
}

#[test]
fn the_snippets_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'snippets'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "the snippets table is missing after migrations");
}

#[test]
fn a_snippet_round_trips_every_field() {
    let store = store();
    let row = store
        .insert_snippet(&snippet("s-1", "Address"))
        .expect("the snippet inserts");
    assert_eq!(row.id.as_str(), "s-1");
    assert_eq!(row.title, "Address");
    assert_eq!(row.body, "The Address body.");
    assert!(row.created_at > 0, "the store stamps created_at itself");
    assert!(
        row.updated_at >= row.created_at,
        "a fresh snippet's touch starts at its creation",
    );
    let listed = store.list_snippets().expect("the list runs");
    assert_eq!(listed, vec![row], "the row read back matches the insert");
}

#[test]
fn the_list_reads_the_most_recently_touched_first() {
    let store = store();
    store
        .insert_snippet(&snippet("s-1", "Address"))
        .expect("the first snippet inserts");
    store
        .insert_snippet(&snippet("s-2", "Thanks"))
        .expect("the second snippet inserts");
    backdate_stamps(&store, "s-2");
    let titles: Vec<String> = store
        .list_snippets()
        .expect("the list runs")
        .into_iter()
        .map(|row| row.title)
        .collect();
    assert_eq!(
        titles,
        vec!["Address".to_string(), "Thanks".to_string()],
        "the fresher touch lists first",
    );
}

#[test]
fn update_rewrites_the_fields_and_touches_updated_at() {
    let store = store();
    store
        .insert_snippet(&snippet("s-1", "Address"))
        .expect("the snippet inserts");
    backdate_stamps(&store, "s-1");
    assert!(
        store
            .update_snippet(
                &SnippetId::new("s-1".to_string()),
                "Home address",
                "12 Rue Pasteur, Paris.",
            )
            .expect("the update runs"),
        "the known snippet reports changed",
    );
    let row = &store.list_snippets().expect("the list runs")[0];
    assert_eq!(row.title, "Home address");
    assert_eq!(row.body, "12 Rue Pasteur, Paris.");
    assert!(
        row.updated_at > row.created_at,
        "the update touched updated_at past the backdated stamp",
    );
    assert!(
        !store
            .update_snippet(
                &SnippetId::new("no-such-snippet".to_string()),
                "Ghost",
                "Nothing.",
            )
            .expect("the update runs"),
        "an unknown id changes nothing and says so",
    );
}

#[test]
fn delete_removes_the_snippet_and_reports_an_unknown_id() {
    let store = store();
    store
        .insert_snippet(&snippet("s-1", "Address"))
        .expect("the snippet inserts");
    assert!(
        store
            .delete_snippet(&SnippetId::new("s-1".to_string()))
            .expect("the delete runs"),
        "the known snippet reports deleted",
    );
    assert!(
        store.list_snippets().expect("the list runs").is_empty(),
        "the snippet is gone",
    );
    assert!(
        !store
            .delete_snippet(&SnippetId::new("s-1".to_string()))
            .expect("the delete runs"),
        "a second delete changes nothing and says so",
    );
}

#[test]
fn generated_snippet_ids_are_distinct_and_nonempty() {
    let first = SnippetId::generate();
    let second = SnippetId::generate();
    assert!(!first.as_str().is_empty());
    assert_ne!(first, second);
}
