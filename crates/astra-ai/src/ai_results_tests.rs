// SPDX-License-Identifier: GPL-3.0-only
//! AI results store tests: the migration lands the table, a result round
//! trips every field with its optional columns both ways, and the query by
//! message answers only that message's rows newest first (the seen check
//! and the CLI's verdict read both ride it). The stage and verdict
//! vocabularies are the app layer's, pinned in its own tests.

use crate::{AiStore, CorrelationId, NewAiResult, RuleId};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One insert with every required field set to a distinct value.
fn result(correlation: &CorrelationId, account: &str, key: &str) -> NewAiResult {
    NewAiResult {
        correlation: correlation.clone(),
        message_account: account.to_owned(),
        message_key: key.to_owned(),
        stage: "rule_match".to_owned(),
        rule_id: Some(RuleId::new("rule-7".to_owned())),
        verdict: "yes".to_owned(),
        category: None,
    }
}

#[test]
fn the_ai_results_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ai_results'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "the ai_results table is missing after migrations");
}

#[test]
fn a_result_round_trips_every_field() {
    let store = store();
    let correlation = CorrelationId::generate();
    store
        .record_ai_result(&result(&correlation, "work", "m1"))
        .expect("the result records");
    let rows = store
        .ai_results_for_message("work", "m1")
        .expect("the query runs");
    assert_eq!(rows.len(), 1, "exactly the one result");
    let row = &rows[0];
    assert_eq!(row.correlation, correlation);
    assert_eq!(row.message_account, "work");
    assert_eq!(row.message_key, "m1");
    assert_eq!(row.stage, "rule_match");
    assert_eq!(row.rule_id.as_ref().map(RuleId::as_str), Some("rule-7"));
    assert_eq!(row.verdict, "yes");
    assert!(row.category.is_none(), "no category was recorded");
    assert!(
        row.created_at > 0,
        "the store stamps the creation time itself"
    );
}

#[test]
fn the_optional_columns_round_trip_both_ways() {
    let store = store();
    let correlation = CorrelationId::generate();
    let mut row = result(&correlation, "work", "m1");
    row.rule_id = None;
    row.category = Some("newsletter".to_owned());
    store.record_ai_result(&row).expect("the result records");
    let read = store
        .ai_results_for_message("work", "m1")
        .expect("the query runs");
    assert_eq!(read.len(), 1);
    assert!(
        read[0].rule_id.is_none(),
        "a stage with no rule stores NULL, not a blank"
    );
    assert_eq!(
        read[0].category.as_deref(),
        Some("newsletter"),
        "a recorded category survives"
    );
}

#[test]
fn the_query_answers_one_messages_rows_newest_first() {
    let store = store();
    let correlation = CorrelationId::generate();
    // Same-second inserts, so the newest-first order falls to the row id.
    for (account, key) in [("work", "m1"), ("work", "m2"), ("home", "m1")] {
        store
            .record_ai_result(&result(&correlation, account, key))
            .expect("the result records");
    }
    store
        .record_ai_result(&result(&correlation, "work", "m1"))
        .expect("the second result for m1 records");
    let rows = store
        .ai_results_for_message("work", "m1")
        .expect("the query runs");
    assert_eq!(rows.len(), 2, "only the one message's rows");
    assert!(
        rows[0].created_at >= rows[1].created_at,
        "the newest result leads"
    );
    assert!(
        store
            .ai_results_for_message("work", "m9")
            .expect("runs")
            .is_empty(),
        "a message with no results answers empty, the pass's seen check"
    );
}
