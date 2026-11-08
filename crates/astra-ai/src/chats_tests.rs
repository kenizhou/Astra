// SPDX-License-Identifier: GPL-3.0-only
//! Chat store tests: the 013 migration lands the tables, a chat round
//! trips every field, the list reads the most recently touched first, a
//! message lands with its parts and metadata in order and touches its
//! chat, a rename rewrites the title, and a delete removes the chat with
//! its messages.

use serde_json::json;

use crate::{AiStore, ChatId, ChatRole, NewChat};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("the in-memory store opens and migrates")
}

/// One chat with every field set, so a round trip cannot pass by
/// coincidence.
fn chat(id: &str, title: &str) -> NewChat {
    NewChat {
        id: ChatId::new(id.to_string()),
        title: title.to_string(),
    }
}

/// Backdates one chat's stamps by an hour, so ordering assertions do not
/// depend on the same-second insert stamps.
fn backdate(store: &AiStore, id: &str) {
    let conn = store.test_conn();
    let conn = conn.lock().expect("the test connection");
    conn.execute(
        "UPDATE chats SET
             created_at = strftime('%s','now') - 3600,
             last_touched_at = strftime('%s','now') - 3600
         WHERE id = ?1",
        [id],
    )
    .expect("the stamps backdate");
}

#[test]
fn the_chats_migration_lands_the_tables() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("the test connection");
    for table in ["chats", "chat_messages"] {
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = '{table}'"
                ),
                [],
                |row| row.get(0),
            )
            .expect("the catalog answers");
        assert_eq!(count, 1, "the {table} table stands");
    }
    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM ai_migrations", [], |row| {
            row.get(0)
        })
        .expect("the migration bookkeeping answers");
    assert!(version >= 13, "the chats migration is embedded: {version}");
}

#[test]
fn a_chat_round_trips_every_field() {
    let store = store();
    let row = store
        .insert_chat(&chat("chat-1", "Friday plans"))
        .expect("the insert lands");
    assert_eq!(row.id.as_str(), "chat-1");
    assert_eq!(row.title, "Friday plans");
    assert!(row.created_at > 0, "the store stamps the creation");
    assert_eq!(
        row.last_touched_at, row.created_at,
        "a fresh chat's touch equals its creation"
    );
    assert!(
        store
            .chat_exists(&ChatId::new("chat-1".to_string()))
            .expect("the existence check runs"),
        "the inserted chat exists"
    );
    assert!(
        !store
            .chat_exists(&ChatId::new("chat-none".to_string()))
            .expect("the existence check runs"),
        "an unknown id exists not"
    );
}

#[test]
fn the_list_reads_the_most_recently_touched_first() {
    let store = store();
    store
        .insert_chat(&chat("chat-old", "Older thread"))
        .expect("the first insert lands");
    store
        .insert_chat(&chat("chat-new", "Newer thread"))
        .expect("the second insert lands");
    backdate(&store, "chat-old");

    let listed = store.list_chats().expect("the list reads");
    assert_eq!(
        listed.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["chat-new", "chat-old"],
        "the touched stamp orders the thread list"
    );
}

#[test]
fn a_message_lands_with_parts_and_metadata_and_touches_its_chat() {
    let store = store();
    let chat = store
        .insert_chat(&chat("chat-7", "Search the inbox"))
        .expect("the chat lands");
    backdate(&store, "chat-7");

    let parts = json!([
        { "kind": "text", "text": "Found three." },
        { "kind": "tool", "name": "search_messages", "status": "ok" },
        { "kind": "note", "text": "the tool budget is spent" },
    ]);
    let metadata = json!({
        "model": "mock-1",
        "tier": "chat",
        "finish_reason": "done",
        "tool_calls": 1,
        "correlation": "corr-7"
    });
    store
        .insert_chat_message(
            &chat.id,
            ChatRole::User,
            &json!([{ "kind": "text", "text": "find it" }]),
            None,
        )
        .expect("the user message lands");
    let assistant = store
        .insert_chat_message(&chat.id, ChatRole::Assistant, &parts, Some(&metadata))
        .expect("the assistant message lands");

    let messages = store.chat_messages(&chat.id).expect("the messages read");
    assert_eq!(messages.len(), 2, "both sides of the turn stand");
    assert_eq!(
        messages[0].id,
        assistant.id - 1,
        "the user side lands first"
    );
    assert_eq!(messages[0].role, "user");
    assert_eq!(
        messages[0].parts,
        json!([{ "kind": "text", "text": "find it" }])
    );
    assert_eq!(
        messages[0].metadata, None,
        "the user side carries no run metadata"
    );
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].parts, parts, "the rendered parts round trip");
    assert_eq!(messages[1].metadata.as_ref(), Some(&metadata));
    assert_eq!(messages[1].chat_id, chat.id, "the message names its chat");

    let listed = store.list_chats().expect("the list reads");
    assert_eq!(
        listed[0].id.as_str(),
        "chat-7",
        "a message insert touches its chat to the front"
    );
    assert!(
        listed[0].last_touched_at > listed[0].created_at,
        "the touch moved past the backdated creation"
    );
}

#[test]
fn a_rename_rewrites_the_title() {
    let store = store();
    store
        .insert_chat(&chat("chat-9", "Working title"))
        .expect("the chat lands");

    let went = store
        .rename_chat(&ChatId::new("chat-9".to_string()), "Renamed thread")
        .expect("the rename runs");
    assert!(went, "a known chat renames");

    let listed = store.list_chats().expect("the list reads");
    assert_eq!(listed[0].title, "Renamed thread");

    let unknown = store
        .rename_chat(&ChatId::new("chat-none".to_string()), "Nothing")
        .expect("the rename runs");
    assert!(!unknown, "an unknown chat reports no row");
}

#[test]
fn a_delete_removes_the_chat_and_its_messages() {
    let store = store();
    let doomed = store
        .insert_chat(&chat("chat-x", "Doomed thread"))
        .expect("the chat lands");
    store
        .insert_chat(&chat("chat-keep", "Kept thread"))
        .expect("the other chat lands");
    store
        .insert_chat_message(
            &doomed.id,
            ChatRole::User,
            &json!([{ "kind": "text", "text": "hi" }]),
            None,
        )
        .expect("the message lands");

    let went = store.delete_chat(&doomed.id).expect("the delete runs");
    assert!(went, "a known chat deletes");
    assert!(
        store
            .chat_messages(&doomed.id)
            .expect("the messages read")
            .is_empty(),
        "the chat's messages went with it"
    );
    assert_eq!(store.list_chats().expect("the list reads").len(), 1);
    assert!(
        !store.delete_chat(&doomed.id).expect("the delete runs"),
        "a second delete reports no row"
    );
}
