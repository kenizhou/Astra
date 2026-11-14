// SPDX-License-Identifier: GPL-3.0-only
//! Store tests: the migration applies and claims once, a definition round
//! trips, the slug derives and suffixes on collision, the name is unique
//! case-insensitively, validation refuses typed, a rename or recolour keeps
//! the assignments (they key the id), a delete cascades its assignments and
//! counts them (proving foreign keys are ON), assign and unassign are
//! idempotent, an unknown category is refused, and the overlay read answers
//! per-thread assignment order, an empty page, and account scoping.

use std::collections::HashMap;

use engine_api::AccountId;

use crate::{CategoryDef, CategoryError, CategoryId, CategoryStore};

/// A migrated in-memory store.
fn store() -> CategoryStore {
    CategoryStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// The owning account the assignment tests use.
fn account() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// A category id from its slug string.
fn id(slug: &str) -> CategoryId {
    CategoryId::new(slug.to_owned())
}

#[test]
fn migrations_create_the_category_tables() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    for table in ["categories", "thread_categories", "category_migrations"] {
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
fn foreign_keys_are_enabled_on_open() {
    // The load-bearing pragma: rusqlite leaves foreign keys off by default,
    // and the delete cascade silently no-ops without them.
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("pragma reads");
    assert_eq!(on, 1, "foreign keys must be ON for the cascade to fire");
}

#[test]
fn a_definition_round_trips() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    assert_eq!(def.id.as_str(), "work");
    assert_eq!(def.name, "Work");
    assert_eq!(def.color, "#FF0000");

    let listed = store.list_categories().expect("lists");
    assert_eq!(listed, vec![def], "the definition reads back whole");
}

#[test]
fn the_slug_lowercases_and_collapses_runs_to_one_dash() {
    let store = store();
    let def = store
        .create_category("  Follow   Up!! Soon  ", "#00FF00")
        .expect("creates");
    assert_eq!(def.id.as_str(), "follow-up-soon");
    assert_eq!(
        def.name, "Follow   Up!! Soon",
        "the name is trimmed, not collapsed"
    );
}

#[test]
fn a_symbol_only_name_reads_as_category() {
    let store = store();
    let def = store.create_category("!!!", "#0000FF").expect("creates");
    assert_eq!(def.id.as_str(), "category");
}

#[test]
fn slug_collisions_suffix_with_a_counter() {
    let store = store();
    // Distinct names that slug to the same base: the id suffixes -2, -3.
    let first = store
        .create_category("Follow Up", "#111111")
        .expect("first");
    let second = store
        .create_category("Follow-Up", "#222222")
        .expect("second");
    let third = store.create_category("follow up", "#333333");
    assert_eq!(first.id.as_str(), "follow-up");
    assert_eq!(second.id.as_str(), "follow-up-2");
    // "follow up" differs from "Follow Up" only by case, so the NAME collides
    // (NOCASE) even though the slug would too: a typed refusal, not a suffix.
    assert!(matches!(third, Err(CategoryError::AlreadyExists(_))));
}

#[test]
fn the_name_is_unique_case_insensitively() {
    let store = store();
    store.create_category("Work", "#FF0000").expect("first");
    let clash = store.create_category("work", "#00FF00");
    assert!(
        matches!(clash, Err(CategoryError::AlreadyExists(name)) if name == "work"),
        "a case-only clash is a typed already-exists refusal"
    );
}

#[test]
fn validation_refuses_an_empty_name() {
    let store = store();
    assert!(matches!(
        store.create_category("   ", "#FF0000"),
        Err(CategoryError::EmptyName)
    ));
}

#[test]
fn validation_refuses_a_name_over_64_characters() {
    let store = store();
    let long = "a".repeat(65);
    assert!(matches!(
        store.create_category(&long, "#FF0000"),
        Err(CategoryError::NameTooLong)
    ));
    // Exactly 64 is accepted.
    let boundary = "b".repeat(64);
    assert!(store.create_category(&boundary, "#FF0000").is_ok());
}

#[test]
fn validation_refuses_a_colour_that_is_not_rrggbb() {
    let store = store();
    for bad in ["FF0000", "#F00", "#GGGGGG", "#FF00000", "red", ""] {
        assert!(
            matches!(
                store.create_category("Work", bad),
                Err(CategoryError::InvalidColor(_))
            ),
            "{bad:?} must be refused as a colour"
        );
    }
    // Lowercase hex is accepted (the shape is hex digits, either case).
    assert!(store.create_category("Work", "#ff00aa").is_ok());
}

#[test]
fn a_rename_or_recolour_keeps_the_assignments() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    store
        .assign_category(&account(), "th-1", &def.id)
        .expect("assigns");

    store
        .update_category(&def.id, "Projects", "#00FF00")
        .expect("renames and recolours");

    let listed = store.list_categories().expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, def.id, "the id is unchanged");
    assert_eq!(listed[0].name, "Projects");
    assert_eq!(listed[0].color, "#00FF00");

    let assignments = store
        .assignments_for_threads(&account(), &["th-1".to_owned()])
        .expect("reads");
    assert_eq!(
        assignments.get("th-1"),
        Some(&vec![def.id.clone()]),
        "the assignment survives the rename (it keys the id)"
    );
}

#[test]
fn update_refuses_an_unknown_id() {
    let store = store();
    assert!(matches!(
        store.update_category(&id("ghost"), "Work", "#FF0000"),
        Err(CategoryError::UnknownCategory(_))
    ));
}

#[test]
fn update_refuses_a_case_insensitive_name_clash() {
    let store = store();
    let work = store.create_category("Work", "#FF0000").expect("work");
    store
        .create_category("Personal", "#00FF00")
        .expect("personal");
    let clash = store.update_category(&work.id, "personal", "#0000FF");
    assert!(matches!(clash, Err(CategoryError::AlreadyExists(_))));
}

#[test]
fn delete_cascades_the_assignments_and_counts_them() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    store
        .assign_category(&account(), "th-1", &def.id)
        .expect("a");
    store
        .assign_category(&account(), "th-2", &def.id)
        .expect("b");
    // A second category on th-1 must survive the first's deletion.
    let other = store.create_category("Personal", "#00FF00").expect("other");
    store
        .assign_category(&account(), "th-1", &other.id)
        .expect("c");

    let removed = store.delete_category(&def.id).expect("deletes");
    assert_eq!(removed, 2, "the two assignments of the deleted definition");

    let assignments = store
        .assignments_for_threads(&account(), &["th-1".to_owned(), "th-2".to_owned()])
        .expect("reads");
    assert_eq!(
        assignments.get("th-1"),
        Some(&vec![other.id.clone()]),
        "the cascade took only the deleted definition's rows"
    );
    assert!(
        !assignments.contains_key("th-2"),
        "th-2's only assignment cascaded away"
    );
    assert_eq!(store.list_categories().expect("lists").len(), 1);
}

#[test]
fn delete_refuses_an_unknown_id() {
    let store = store();
    assert!(matches!(
        store.delete_category(&id("ghost")),
        Err(CategoryError::UnknownCategory(_))
    ));
}

#[test]
fn assign_is_idempotent_and_reports_the_insert() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    assert!(
        store
            .assign_category(&account(), "th-1", &def.id)
            .expect("first"),
        "the first assign inserts"
    );
    assert!(
        !store
            .assign_category(&account(), "th-1", &def.id)
            .expect("second"),
        "a repeat assign inserts nothing"
    );
    let assignments = store
        .assignments_for_threads(&account(), &["th-1".to_owned()])
        .expect("reads");
    assert_eq!(assignments.get("th-1"), Some(&vec![def.id.clone()]));
}

#[test]
fn assign_refuses_an_unknown_category() {
    let store = store();
    assert!(matches!(
        store.assign_category(&account(), "th-1", &id("ghost")),
        Err(CategoryError::UnknownCategory(_))
    ));
}

#[test]
fn unassign_is_idempotent_and_reports_the_removal() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    store
        .assign_category(&account(), "th-1", &def.id)
        .expect("assigns");
    assert!(
        store
            .unassign_category(&account(), "th-1", &def.id)
            .expect("first"),
        "the first unassign removes"
    );
    assert!(
        !store
            .unassign_category(&account(), "th-1", &def.id)
            .expect("second"),
        "a repeat unassign removes nothing"
    );
}

#[test]
fn list_categories_answers_creation_order_and_a_rename_does_not_reorder() {
    let store = store();
    let a = store.create_category("Alpha", "#111111").expect("a");
    let b = store.create_category("Beta", "#222222").expect("b");
    let c = store.create_category("Gamma", "#333333").expect("c");
    store
        .update_category(&a.id, "Alpha Renamed", "#444444")
        .expect("rename");
    let listed = store.list_categories().expect("lists");
    let ids: Vec<CategoryId> = listed.into_iter().map(|def| def.id).collect();
    assert_eq!(ids, vec![a.id, b.id, c.id], "rowid order survives a rename");
}

#[test]
fn assignments_answer_per_thread_rowid_order() {
    let store = store();
    let a = store.create_category("Alpha", "#111111").expect("a");
    let b = store.create_category("Beta", "#222222").expect("b");
    let c = store.create_category("Gamma", "#333333").expect("c");
    // Assign out of creation order; the read answers assignment (rowid) order.
    store.assign_category(&account(), "th-1", &c.id).expect("c");
    store.assign_category(&account(), "th-1", &a.id).expect("a");
    store.assign_category(&account(), "th-1", &b.id).expect("b");
    let assignments = store
        .assignments_for_threads(&account(), &["th-1".to_owned()])
        .expect("reads");
    assert_eq!(
        assignments.get("th-1"),
        Some(&vec![c.id, a.id, b.id]),
        "assignment order, not creation order"
    );
}

#[test]
fn assignments_answer_an_empty_page_with_an_empty_map() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    store
        .assign_category(&account(), "th-1", &def.id)
        .expect("assigns");
    let empty: HashMap<String, Vec<CategoryId>> = store
        .assignments_for_threads(&account(), &[])
        .expect("reads");
    assert!(empty.is_empty(), "an empty page reads nothing");
}

#[test]
fn assignments_are_account_scoped_and_skip_threads_off_the_page() {
    let store = store();
    let def = store.create_category("Work", "#FF0000").expect("creates");
    let other_account = AccountId::try_from("personal").unwrap();
    store
        .assign_category(&account(), "th-1", &def.id)
        .expect("work");
    store
        .assign_category(&other_account, "th-1", &def.id)
        .expect("personal");
    store
        .assign_category(&account(), "th-2", &def.id)
        .expect("off page");

    let assignments = store
        .assignments_for_threads(&account(), &["th-1".to_owned()])
        .expect("reads");
    assert_eq!(assignments.len(), 1, "only the page's thread answers");
    assert_eq!(assignments.get("th-1"), Some(&vec![def.id.clone()]));

    let personal = store
        .assignments_for_threads(&other_account, &["th-1".to_owned()])
        .expect("reads");
    assert_eq!(
        personal.get("th-1"),
        Some(&vec![def.id]),
        "the same thread id under another account is its own assignment"
    );
}

#[test]
fn a_file_store_round_trips_and_enables_foreign_keys() {
    let dir = std::env::temp_dir().join(format!(
        "astra-categories-file-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("categories.sqlite");
    let store = CategoryStore::open(&path).expect("file store opens");
    let def = store.create_category("Work", "#FF0000").expect("creates");
    store
        .assign_category(&account(), "th-1", &def.id)
        .expect("assigns");
    let removed = store.delete_category(&def.id).expect("deletes");
    assert_eq!(removed, 1, "the file store cascades too");
    let listed: Vec<CategoryDef> = store.list_categories().expect("lists");
    assert!(listed.is_empty(), "the definition is gone");
    let _ = std::fs::remove_dir_all(&dir);
}
