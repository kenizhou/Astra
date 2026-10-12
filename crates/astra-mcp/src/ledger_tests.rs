// SPDX-License-Identifier: GPL-3.0-only
//! Ledger tests: the argument character counter (characters, not bytes,
//! recursing through the arguments' shape) and the store-backed sink that
//! lands one row per record.

use astra_ai::{AiStore, CorrelationId};

use crate::ledger::{arg_chars, ToolCallRecord, ToolLedger};

#[test]
fn arg_chars_counts_characters_not_bytes() {
    let args = serde_json::json!({"query": "café ☕"});
    assert_eq!("café ☕".chars().count(), 6, "the fixture itself");
    assert_ne!("café ☕".len(), 6, "and its byte length differs");
    assert_eq!(arg_chars(&args), 6);
}

#[test]
fn arg_chars_sums_every_string_and_recurses() {
    let args = serde_json::json!({
        "account": "work",
        "folder": "inbox",
        "offset": 40,
        "flag": true,
        "list": ["ab", "cde"],
        "nested": {"inner": "f"},
    });
    assert_eq!(arg_chars(&args), 4 + 5 + 2 + 3 + 1);
}

#[test]
fn arg_chars_of_nothing_is_zero() {
    for args in [
        serde_json::json!({}),
        serde_json::json!(null),
        serde_json::json!(17),
        serde_json::json!(false),
    ] {
        assert_eq!(arg_chars(&args), 0, "{args}");
    }
}

#[test]
fn the_store_backed_sink_lands_one_row_per_record() {
    let store = AiStore::open_in_memory().expect("the ledger opens");
    let record = ToolCallRecord {
        correlation: CorrelationId::generate(),
        tool: "search_messages".to_owned(),
        outcome: crate::ledger::Outcome::Refused,
        duration_ms: 31,
        arg_chars: 29,
    };
    ToolLedger::record(&store, &record).expect("the row lands");

    let rows = store.recent_tool_calls(5).expect("the ledger reads");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tool, "search_messages");
    assert_eq!(rows[0].outcome, "refused");
    assert_eq!(rows[0].duration_ms, 31);
    assert_eq!(rows[0].arg_chars, 29);
    assert_eq!(rows[0].correlation, record.correlation);
}
