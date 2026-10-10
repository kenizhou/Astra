// SPDX-License-Identifier: GPL-3.0-only
//! Draft pool store tests: the 0004 migration lands the pool and the
//! style profile sibling, a drafted row round trips every field, a
//! skipped row round trips its nulls, the query by message answers only
//! that message's rows newest first, and a style profile saves, reads
//! back, and replaces per account.

use crate::{AiStore, CorrelationId, NewDraftPoolRow};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One drafted pool row: every field set to a distinct value.
fn drafted(correlation: &CorrelationId, account: &str, key: &str) -> NewDraftPoolRow {
    NewDraftPoolRow {
        correlation: correlation.clone(),
        message_account: account.to_owned(),
        message_key: key.to_owned(),
        reply: Some("Yes, Friday works. See you then.".to_owned()),
        confidence: Some(0.6),
    }
}

/// One skipped pool row: the failure shape, neither half carried.
fn skipped(correlation: &CorrelationId, account: &str, key: &str) -> NewDraftPoolRow {
    NewDraftPoolRow {
        correlation: correlation.clone(),
        message_account: account.to_owned(),
        message_key: key.to_owned(),
        reply: None,
        confidence: None,
    }
}

#[test]
fn the_draft_pool_migration_lands_both_tables() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    for table in ["draft_pool", "style_profiles"] {
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
fn a_drafted_row_round_trips_every_field() {
    let store = store();
    let correlation = CorrelationId::generate();
    let landed = store
        .insert_draft_pool(&drafted(&correlation, "work", "m1"))
        .expect("the drafted row stores");
    assert!(
        landed.created_at > 0,
        "the insert answers the store's own creation stamp"
    );
    assert_eq!(landed.correlation, correlation);
    assert_eq!(landed.message_account, "work");
    assert_eq!(landed.message_key, "m1");
    assert_eq!(
        landed.reply.as_deref(),
        Some("Yes, Friday works. See you then.")
    );
    assert_eq!(landed.confidence, Some(0.6));
    let rows = store
        .draft_pool_for_message("work", "m1")
        .expect("the query runs");
    assert_eq!(rows.len(), 1, "exactly the one pool row");
    assert_eq!(rows[0], landed, "the read back matches the landed row");
}

#[test]
fn a_skipped_row_round_trips_its_nulls() {
    let store = store();
    store
        .insert_draft_pool(&skipped(&CorrelationId::generate(), "work", "m2"))
        .expect("the skipped row stores");
    let rows = store
        .draft_pool_for_message("work", "m2")
        .expect("the query runs");
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].reply.is_none(),
        "a skipped row stores NULL, not a blank"
    );
    assert!(rows[0].confidence.is_none());
}

#[test]
fn the_query_answers_one_messages_rows_newest_first() {
    let store = store();
    let correlation = CorrelationId::generate();
    // Same-second inserts, so the newest-first order falls to the row id.
    for (account, key) in [("work", "m1"), ("work", "m2"), ("home", "m1")] {
        store
            .insert_draft_pool(&drafted(&correlation, account, key))
            .expect("the pool row stores");
    }
    let first = store
        .insert_draft_pool(&skipped(&correlation, "work", "m1"))
        .expect("the second row for m1 stores");
    let rows = store
        .draft_pool_for_message("work", "m1")
        .expect("the query runs");
    assert_eq!(rows.len(), 2, "only the one message's rows");
    assert_eq!(rows[0], first, "the newest row leads");
    assert!(
        rows[0].created_at >= rows[1].created_at,
        "creation time backs the row id's order"
    );
    assert!(
        store
            .draft_pool_for_message("work", "m9")
            .expect("runs")
            .is_empty(),
        "a message never drafted answers empty"
    );
}

#[test]
fn a_style_profile_saves_reads_back_and_replaces() {
    let store = store();
    assert!(
        store
            .style_profile("work")
            .expect("the read runs")
            .is_none(),
        "an account with no profile answers none"
    );
    store
        .save_style_profile("work", "Concise and warm; signs off with Cheers.")
        .expect("the profile saves");
    let first = store
        .style_profile("work")
        .expect("the read runs")
        .expect("the saved profile reads back");
    assert_eq!(first.account, "work");
    assert_eq!(first.note, "Concise and warm; signs off with Cheers.");
    assert!(first.built_at > 0, "the store stamps the build time itself");
    store
        .save_style_profile("work", "Short declarative sentences.")
        .expect("the rebuild saves");
    let second = store
        .style_profile("work")
        .expect("the read runs")
        .expect("one row per account, not two");
    assert_eq!(second.note, "Short declarative sentences.");
    assert!(
        second.built_at >= first.built_at,
        "the rebuild's stamp never moves backwards"
    );
    assert!(
        store.style_profile("home").expect("runs").is_none(),
        "another account's profile stays its own row"
    );
}
