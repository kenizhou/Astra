// SPDX-License-Identifier: GPL-3.0-only
//! Tool-call ledger tests: the table's migration, the round trip with every
//! field intact, the store-stamped creation time, the newest-first read, and
//! the never-content pin at the column-shape level.

use crate::{AiStore, CorrelationId, NewToolCall};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One tool call to record: every field set to a distinct value so a round
/// trip cannot pass by coincidence.
fn call(tool: &str, outcome: &str, arg_chars: u64) -> NewToolCall {
    NewToolCall {
        correlation: CorrelationId::generate(),
        tool: tool.to_owned(),
        outcome: outcome.to_owned(),
        duration_ms: 4_711,
        arg_chars,
    }
}

#[test]
fn migrations_create_the_tool_calls_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'tool_calls'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "tool_calls missing after migrations");
}

#[test]
fn tool_call_round_trips_every_field() {
    let store = store();
    let call = call("search_messages", "ok", 29);
    let correlation = call.correlation.clone();
    let landed = store
        .record_tool_call(&call)
        .expect("the tool call records");

    assert_eq!(landed.correlation, correlation);
    assert_eq!(landed.tool, "search_messages");
    assert_eq!(landed.outcome, "ok");
    assert_eq!(landed.duration_ms, 4_711);
    assert_eq!(landed.arg_chars, 29);

    let read = store.recent_tool_calls(10).expect("the read runs");
    assert_eq!(read.len(), 1, "one row lands per call");
    assert_eq!(read[0].correlation, correlation);
    assert_eq!(read[0].tool, "search_messages");
    assert_eq!(read[0].outcome, "ok");
    assert_eq!(read[0].duration_ms, 4_711);
    assert_eq!(read[0].arg_chars, 29);
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
        .record_tool_call(&call("list_accounts", "ok", 0))
        .expect("the tool call records");
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
fn recent_tool_calls_orders_newest_first() {
    let store = store();
    store
        .record_tool_call(&call("list_accounts", "ok", 0))
        .expect("the first records");
    store
        .record_tool_call(&call("get_message", "ok", 44))
        .expect("the second records");

    let read = store.recent_tool_calls(10).expect("the read runs");
    assert_eq!(
        read.iter().map(|row| row.tool.as_str()).collect::<Vec<_>>(),
        ["get_message", "list_accounts"],
        "newest first, the insertion order reversed"
    );
    let limited = store.recent_tool_calls(1).expect("the limited read runs");
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].tool, "get_message");
}

/// The never-content pin at the column level: the table holds exactly the
/// counted columns and nothing that could carry a query, an address, a
/// subject, or a body. A column that does not exist cannot be filled by a
/// later change that forgets the rule.
#[test]
fn the_table_shape_holds_no_content_column() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let mut statement = conn
        .prepare("PRAGMA table_info(tool_calls)")
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
        [
            "arg_chars",
            "correlation_id",
            "created_at",
            "duration_ms",
            "id",
            "outcome",
            "tool",
        ],
        "the tool_calls columns are counts, ids, and names only"
    );
}
