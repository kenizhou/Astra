// SPDX-License-Identifier: GPL-3.0-only
//! Text cache store tests: the 009 migration lands the table, a put
//! hits only under its full key (message pair, kind, lang, body hash),
//! a lang-null row coexists with lang-set rows of the same message, a
//! re-put replaces the row for its key whether the lang is null or
//! set, and each message keeps its own entries.

use crate::AiStore;

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// The rows one key holds, for the replacement assertions.
fn row_count(store: &AiStore, account: &str, key: &str) -> i64 {
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    conn.query_row(
        "SELECT COUNT(*) FROM text_cache WHERE message_account = ?1 AND message_key = ?2",
        [account, key],
        |row| row.get(0),
    )
    .expect("the count reads")
}

#[test]
fn the_text_cache_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'text_cache'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "the text_cache table is missing after migrations");
}

#[test]
fn a_put_hits_only_under_its_full_key() {
    let store = store();
    store
        .cache_text("work", "m1", "translate", Some("de"), "hash-a", "Hallo.")
        .expect("the text caches");
    for (kind, lang, body_hash) in [
        ("translate", Some("de"), "hash-a"),
        // Every other key misses, each dimension on its own.
        ("translate", Some("fr"), "hash-a"),
        ("translate", None, "hash-a"),
        ("summary", Some("de"), "hash-a"),
        ("translate", Some("de"), "hash-b"),
    ] {
        assert_eq!(
            store
                .cached_text("work", "m1", kind, lang, body_hash)
                .expect("the read runs"),
            if (kind, lang, body_hash) == ("translate", Some("de"), "hash-a") {
                Some("Hallo.".to_string())
            } else {
                None
            },
            "the ({kind}, {lang:?}, {body_hash}) key must not borrow another key's row",
        );
    }
    assert!(
        store
            .cached_text("home", "m1", "translate", Some("de"), "hash-a")
            .expect("the read runs")
            .is_none(),
        "another account's cache stays its own row",
    );
}

#[test]
fn a_lang_null_row_coexists_with_lang_set_rows_of_the_same_message() {
    let store = store();
    store
        .cache_text(
            "work",
            "m1",
            "translate",
            None,
            "hash-a",
            "The English text.",
        )
        .expect("the langless row caches");
    store
        .cache_text(
            "work",
            "m1",
            "translate",
            Some("de"),
            "hash-a",
            "Der deutsche Text.",
        )
        .expect("the German row caches");
    store
        .cache_text(
            "work",
            "m1",
            "translate",
            Some("fr"),
            "hash-a",
            "Le texte francais.",
        )
        .expect("the French row caches");
    assert_eq!(
        store
            .cached_text("work", "m1", "translate", None, "hash-a")
            .expect("the read runs"),
        Some("The English text.".to_string()),
        "the langless row answers under its null lang",
    );
    assert_eq!(
        store
            .cached_text("work", "m1", "translate", Some("de"), "hash-a")
            .expect("the read runs"),
        Some("Der deutsche Text.".to_string()),
        "the German row answers under its lang",
    );
    assert_eq!(
        store
            .cached_text("work", "m1", "translate", Some("fr"), "hash-a")
            .expect("the read runs"),
        Some("Le texte francais.".to_string()),
        "the French row answers under its lang",
    );
    assert_eq!(
        row_count(&store, "work", "m1"),
        3,
        "the null lang keys apart from the set langs, one row per key",
    );
}

#[test]
fn a_re_put_replaces_the_row_for_its_key_whatever_the_lang() {
    let store = store();
    store
        .cache_text(
            "work",
            "m1",
            "translate",
            Some("de"),
            "hash-a",
            "Der erste Text.",
        )
        .expect("the German row caches");
    store
        .cache_text(
            "work",
            "m1",
            "translate",
            Some("de"),
            "hash-b",
            "Der zweite Text.",
        )
        .expect("the re-put under the new hash stores");
    assert!(
        store
            .cached_text("work", "m1", "translate", Some("de"), "hash-a")
            .expect("the read runs")
            .is_none(),
        "the old hash no longer hits",
    );
    assert_eq!(
        store
            .cached_text("work", "m1", "translate", Some("de"), "hash-b")
            .expect("the read runs"),
        Some("Der zweite Text.".to_string()),
        "one row per key, the newest put's content",
    );
    assert_eq!(
        row_count(&store, "work", "m1"),
        1,
        "no stack behind the key"
    );
    store
        .cache_text("work", "m1", "translate", None, "hash-a", "The first text.")
        .expect("the langless row caches");
    store
        .cache_text(
            "work",
            "m1",
            "translate",
            None,
            "hash-a",
            "The second text.",
        )
        .expect("the langless re-put stores");
    assert_eq!(
        store
            .cached_text("work", "m1", "translate", None, "hash-a")
            .expect("the read runs"),
        Some("The second text.".to_string()),
        "the langless re-put replaced its row",
    );
    assert_eq!(
        row_count(&store, "work", "m1"),
        2,
        "one row per key across the null and the set lang",
    );
}

#[test]
fn each_message_keeps_its_own_entries() {
    let store = store();
    for (account, key, text) in [
        ("work", "m1", "One."),
        ("work", "m2", "Two."),
        ("home", "m1", "Three."),
    ] {
        store
            .cache_text(account, key, "translate", None, "hash-a", text)
            .expect("the row caches");
    }
    for (account, key, expected) in [
        ("work", "m1", "One."),
        ("work", "m2", "Two."),
        ("home", "m1", "Three."),
    ] {
        assert_eq!(
            store
                .cached_text(account, key, "translate", None, "hash-a")
                .expect("the read runs"),
            Some(expected.to_string()),
            "the ({account}, {key}) pair keeps its own row",
        );
    }
}
