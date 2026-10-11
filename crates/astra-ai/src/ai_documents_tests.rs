// SPDX-License-Identifier: GPL-3.0-only
//! AI documents store tests: the 010 migration lands the table, a put
//! round trips every field through the latest read, a put for the same
//! (kind, period) key replaces the row and moves `created_at`, kinds
//! and periods key apart inside one account, the account list reads
//! newest first and stays per account, and the kind's wire words stay
//! pinned.

use crate::{AiDocumentKind, AiStore, CorrelationId, NewAiDocument};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One document with every field set to a distinct value so a round
/// trip cannot pass by coincidence.
fn document(
    account: &str,
    kind: AiDocumentKind,
    period_start: i64,
    period_end: i64,
    content: &str,
) -> NewAiDocument {
    NewAiDocument {
        account: account.to_string(),
        kind,
        period_start,
        period_end,
        content: content.to_string(),
        correlation: CorrelationId::new("corr-7".to_string()),
    }
}

/// The rows one (account, kind, period) key holds, for the replacement
/// assertions.
fn row_count(
    store: &AiStore,
    account: &str,
    kind: AiDocumentKind,
    period_start: i64,
    period_end: i64,
) -> i64 {
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    conn.query_row(
        "SELECT COUNT(*) FROM ai_documents
         WHERE account = ?1 AND kind = ?2 AND period_start = ?3 AND period_end = ?4",
        rusqlite::params![account, kind.as_str(), period_start, period_end],
        |row| row.get(0),
    )
    .expect("the count reads")
}

#[test]
fn the_ai_documents_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ai_documents'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(
        found, 1,
        "the ai_documents table is missing after migrations"
    );
}

#[test]
fn a_document_round_trips_every_field() {
    let store = store();
    assert!(
        store
            .latest_document("work", AiDocumentKind::Digest)
            .expect("the latest read runs")
            .is_none(),
        "an account with no document answers none",
    );
    let row = store
        .put_document(&document(
            "work",
            AiDocumentKind::Digest,
            1_700_000_000,
            1_700_605_200,
            "The week in one page.",
        ))
        .expect("the document puts");
    assert!(row.id > 0, "the store mints the id");
    assert_eq!(row.account, "work");
    assert_eq!(row.kind, AiDocumentKind::Digest);
    assert_eq!(row.period_start, 1_700_000_000);
    assert_eq!(row.period_end, 1_700_605_200);
    assert_eq!(row.content, "The week in one page.");
    assert_eq!(row.correlation.as_str(), "corr-7");
    assert!(row.created_at > 0, "the store stamps created_at itself");
    let latest = store
        .latest_document("work", AiDocumentKind::Digest)
        .expect("the latest read runs")
        .expect("the put row answers the latest read");
    assert_eq!(latest, row, "the row read back matches the put");
    let listed = store.documents_for("work").expect("the list runs");
    assert_eq!(listed, vec![row], "the list serves the same row");
}

#[test]
fn a_same_key_put_replaces_the_row_and_moves_created_at() {
    let store = store();
    let first = store
        .put_document(&document(
            "work",
            AiDocumentKind::Report,
            100,
            200,
            "The first report.",
        ))
        .expect("the first report puts");
    let backdated = first.created_at - 3600;
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        conn.execute(
            "UPDATE ai_documents SET created_at = ?2 WHERE id = ?1",
            rusqlite::params![first.id, backdated],
        )
        .expect("the first report is backdated an hour");
    }
    let second = store
        .put_document(&document(
            "work",
            AiDocumentKind::Report,
            100,
            200,
            "The regenerated report.",
        ))
        .expect("the same key re-puts");
    assert_eq!(second.content, "The regenerated report.");
    assert!(
        second.created_at > backdated,
        "the replacement's stamp moved past the backdated one",
    );
    assert!(
        second.created_at >= first.created_at,
        "the replacement's stamp is never behind the first put's",
    );
    assert_eq!(
        row_count(&store, "work", AiDocumentKind::Report, 100, 200),
        1,
        "no stack behind the key",
    );
    assert_eq!(
        store
            .latest_document("work", AiDocumentKind::Report)
            .expect("the latest read runs")
            .expect("the key's row answers"),
        second,
        "the latest read serves the replacement",
    );
}

#[test]
fn kinds_and_periods_key_apart_inside_one_account() {
    let store = store();
    store
        .put_document(&document(
            "work",
            AiDocumentKind::Digest,
            100,
            200,
            "The digest.",
        ))
        .expect("the digest puts");
    store
        .put_document(&document(
            "work",
            AiDocumentKind::Report,
            100,
            200,
            "The report.",
        ))
        .expect("the same-period report puts");
    store
        .put_document(&document(
            "work",
            AiDocumentKind::Digest,
            300,
            400,
            "The later digest.",
        ))
        .expect("the later period's digest puts");
    assert_eq!(
        store
            .latest_document("work", AiDocumentKind::Report)
            .expect("the latest read runs")
            .expect("the report answers")
            .content,
        "The report.",
        "the same period's digest row never answers the report kind",
    );
    assert_eq!(
        store
            .latest_document("work", AiDocumentKind::Digest)
            .expect("the latest read runs")
            .expect("a digest answers")
            .content,
        "The later digest.",
        "the kind's latest is the newest generation, period apart",
    );
    assert_eq!(
        row_count(&store, "work", AiDocumentKind::Digest, 100, 200),
        1,
        "one row behind the first period",
    );
    assert_eq!(
        row_count(&store, "work", AiDocumentKind::Digest, 300, 400),
        1,
        "one row behind the second period: a changed window is its own row",
    );
    assert_eq!(
        store.documents_for("work").expect("the list runs").len(),
        3,
        "every key keeps its own row",
    );
}

#[test]
fn the_account_list_reads_newest_first_and_stays_per_account() {
    let store = store();
    store
        .put_document(&document(
            "work",
            AiDocumentKind::Digest,
            100,
            200,
            "The older digest.",
        ))
        .expect("the older digest puts");
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        conn.execute(
            "UPDATE ai_documents SET created_at = strftime('%s','now') - 3600
             WHERE kind = 'digest'",
            [],
        )
        .expect("the older digest is backdated an hour");
    }
    store
        .put_document(&document(
            "work",
            AiDocumentKind::Report,
            100,
            200,
            "The newer report.",
        ))
        .expect("the newer report puts");
    store
        .put_document(&document(
            "home",
            AiDocumentKind::Digest,
            100,
            200,
            "Another account's digest.",
        ))
        .expect("another account's digest puts");
    let contents: Vec<String> = store
        .documents_for("work")
        .expect("the list runs")
        .into_iter()
        .map(|row| row.content)
        .collect();
    assert_eq!(
        contents,
        vec![
            "The newer report.".to_string(),
            "The older digest.".to_string()
        ],
        "newest first, and another account's documents stay out",
    );
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        conn.execute(
            "UPDATE ai_documents SET created_at = strftime('%s','now') - 7200
             WHERE account = 'work'",
            [],
        )
        .expect("the work rows' stamps are tied");
    }
    let contents: Vec<String> = store
        .documents_for("work")
        .expect("the list runs")
        .into_iter()
        .map(|row| row.content)
        .collect();
    assert_eq!(
        contents,
        vec![
            "The newer report.".to_string(),
            "The older digest.".to_string()
        ],
        "a same-second tie breaks on the id, the later insert first",
    );
}

#[test]
fn the_kind_wire_words_are_pinned() {
    assert_eq!(AiDocumentKind::Digest.as_str(), "digest");
    assert_eq!(AiDocumentKind::Report.as_str(), "report");
    assert_eq!(
        serde_json::to_string(&AiDocumentKind::Digest).expect("the kind serialises"),
        "\"digest\"",
        "the serde wire word is the snake_case one",
    );
    assert_eq!(
        AiDocumentKind::from_wire("digest"),
        Some(AiDocumentKind::Digest)
    );
    assert_eq!(
        AiDocumentKind::from_wire("report"),
        Some(AiDocumentKind::Report)
    );
    assert_eq!(
        AiDocumentKind::from_wire("digests"),
        None,
        "a word outside the closed set parses to none",
    );
}
