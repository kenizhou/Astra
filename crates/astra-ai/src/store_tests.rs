// SPDX-License-Identifier: GPL-3.0-only
//! Store tests: migrations apply, and llm_call and audit rows round trip
//! with every field intact, the correlation id first among them.

use crate::{AiStore, AuditRow, CorrelationId, LlmCallRow, LlmUseCase, ModelTier};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// A full LLM call row: every field set to a distinct value so a round trip
/// cannot pass by coincidence.
fn llm_call(correlation: &CorrelationId) -> LlmCallRow {
    LlmCallRow {
        correlation: correlation.clone(),
        use_case: LlmUseCase::DraftReply,
        tier: ModelTier::Draft,
        model: "claude-sonnet-5".to_string(),
        prompt_tokens: 1201,
        completion_tokens: 302,
        cost_micros: Some(4096),
        latency_ms: 8012,
        outcome: "ok".to_string(),
    }
}

#[test]
fn migrations_create_both_ledger_tables() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    for table in ["llm_calls", "audit_log", "ai_migrations"] {
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
fn llm_call_round_trips_every_field() {
    let store = store();
    let correlation = CorrelationId::generate();
    store
        .record_llm_call(&llm_call(&correlation))
        .expect("llm call records");
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let row = conn
        .query_row(
            "SELECT correlation_id, use_case, tier, model, prompt_tokens, \
             completion_tokens, cost_micros, latency_ms, outcome FROM llm_calls",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .expect("llm call reads back");
    assert_eq!(row.0, correlation.to_string());
    assert_eq!(row.1, "draft_reply");
    assert_eq!(row.2, "draft");
    assert_eq!(row.3, "claude-sonnet-5");
    assert_eq!(row.4, 1201);
    assert_eq!(row.5, 302);
    assert_eq!(row.6, Some(4096));
    assert_eq!(row.7, 8012);
    assert_eq!(row.8, "ok");
}

#[test]
fn llm_call_without_cost_stores_null() {
    let store = store();
    let row = LlmCallRow {
        cost_micros: None,
        outcome: "refused".to_string(),
        ..llm_call(&CorrelationId::generate())
    };
    store.record_llm_call(&row).expect("llm call records");
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let row = conn
        .query_row("SELECT cost_micros, outcome FROM llm_calls", [], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, String>(1)?))
        })
        .expect("reads back");
    assert_eq!(row.0, None);
    assert_eq!(row.1, "refused");
}

#[test]
fn audit_round_trips_every_field() {
    let store = store();
    let correlation = CorrelationId::generate();
    store
        .record_audit(&AuditRow {
            correlation: correlation.clone(),
            verb: "drafts save".to_string(),
            arg_preview: "{\"draft_id\":\"d-1\"}".to_string(),
            outcome: "ok".to_string(),
        })
        .expect("audit records");
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let row = conn
        .query_row(
            "SELECT correlation_id, verb, arg_preview, outcome FROM audit_log",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .expect("audit reads back");
    assert_eq!(row.0, correlation.to_string());
    assert_eq!(row.1, "drafts save");
    assert_eq!(row.2, "{\"draft_id\":\"d-1\"}");
    assert_eq!(row.3, "ok");
}

#[test]
fn rows_append_without_overwriting() {
    let store = store();
    store
        .record_llm_call(&llm_call(&CorrelationId::generate()))
        .expect("first llm call");
    store
        .record_llm_call(&llm_call(&CorrelationId::generate()))
        .expect("second llm call");
    store
        .record_audit(&AuditRow {
            correlation: CorrelationId::generate(),
            verb: "messages list".to_string(),
            arg_preview: "{}".to_string(),
            outcome: "ok".to_string(),
        })
        .expect("audit");
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let calls: i64 = conn
        .query_row("SELECT COUNT(*) FROM llm_calls", [], |row| row.get(0))
        .expect("count llm calls");
    let audits: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .expect("count audits");
    assert_eq!(calls, 2);
    assert_eq!(audits, 1);
}

#[test]
fn open_creates_the_file_and_reopens_with_the_rows_intact() {
    let dir = std::env::temp_dir().join(format!("astra-ai-store-{}", CorrelationId::generate()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("ai.sqlite3");
    {
        let store = AiStore::open(&path).expect("file store opens");
        store
            .record_audit(&AuditRow {
                correlation: CorrelationId::generate(),
                verb: "accounts list".to_string(),
                arg_preview: "{}".to_string(),
                outcome: "ok".to_string(),
            })
            .expect("audit records");
    }
    let reopened = AiStore::open(&path).expect("store reopens");
    let conn = reopened.test_conn();
    let conn = conn.lock().expect("test connection");
    let audits: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .expect("count audits after reopen");
    assert_eq!(audits, 1);
    std::fs::remove_dir_all(&dir).ok();
}

/// The bookkeeping rows of one store, in order: `ai_migrations` must hold
/// each embedded version exactly once.
fn versions(store: &AiStore) -> Vec<i64> {
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let mut statement = conn
        .prepare("SELECT version FROM ai_migrations ORDER BY version")
        .expect("the bookkeeping query prepares");
    statement
        .query_map([], |row| row.get(0))
        .expect("the bookkeeping query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("the bookkeeping rows read")
}

#[test]
fn the_bookkeeping_row_lands_once_and_survives_the_reopen() {
    let dir = std::env::temp_dir().join(format!("astra-ai-store-{}", CorrelationId::generate()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("ai.sqlite3");
    {
        let store = AiStore::open(&path).expect("the first open migrates");
        assert_eq!(
            versions(&store),
            vec![1, 2, 3, 4, 5, 6],
            "one row per applied migration"
        );
    }
    let reopened = AiStore::open(&path).expect("the reopen claims nothing new");
    assert_eq!(
        versions(&reopened),
        vec![1, 2, 3, 4, 5, 6],
        "the reopen records no second bookkeeping row"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn concurrent_first_opens_all_succeed_with_one_bookkeeping_row() {
    let dir = std::env::temp_dir().join(format!("astra-ai-store-{}", CorrelationId::generate()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("ai.sqlite3");
    std::thread::scope(|scope| {
        let openers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| AiStore::open(&path)))
            .collect();
        for opener in openers {
            opener
                .join()
                .expect("the opener thread ends")
                .expect("a concurrent first open never fails on the bookkeeping row");
        }
    });
    let store = AiStore::open(&path).expect("the store opens after the race");
    assert_eq!(
        versions(&store),
        vec![1, 2, 3, 4, 5, 6],
        "the race applied each migration exactly once"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A nano-tier ledger row with a reported cost, the shape the spend guard
/// aggregates.
fn nano_call(cost: Option<u64>, outcome: &str) -> LlmCallRow {
    LlmCallRow {
        correlation: CorrelationId::generate(),
        use_case: LlmUseCase::JudgeColdEmail,
        tier: ModelTier::Nano,
        model: "gpt-nano-mini".to_string(),
        prompt_tokens: 10,
        completion_tokens: 1,
        cost_micros: cost,
        latency_ms: 90,
        outcome: outcome.to_string(),
    }
}

#[test]
fn weekly_nano_spend_aggregates_nano_rows_of_the_trailing_week() {
    let store = store();
    for cost in [1200u64, 3400] {
        store
            .record_llm_call(&nano_call(Some(cost), "ok"))
            .expect("nano call records");
    }
    store
        .record_llm_call(&nano_call(None, "error:transport"))
        .expect("costless nano call records");
    store
        .record_llm_call(&LlmCallRow {
            use_case: LlmUseCase::JudgeRuleMatch,
            tier: ModelTier::Economy,
            model: "gpt-4o-mini".to_string(),
            outcome: "ok".to_string(),
            ..nano_call(Some(9900), "ok")
        })
        .expect("economy call records");
    assert_eq!(
        store
            .weekly_nano_spend_micros()
            .expect("the weekly aggregate runs"),
        4600,
        "nano costs of the trailing week, costless and non-nano rows aside",
    );
}

#[test]
fn rows_outside_the_trailing_week_do_not_count_against_the_budget() {
    let store = store();
    store
        .record_llm_call(&nano_call(Some(5000), "ok"))
        .expect("nano call records");
    {
        let conn = store.test_conn();
        let conn = conn.lock().expect("test connection");
        conn.execute(
            "UPDATE llm_calls SET created_at = strftime('%s','now') - 691200",
            [],
        )
        .expect("the row is backdated eight days, beyond the week");
    }
    assert_eq!(
        store
            .weekly_nano_spend_micros()
            .expect("the weekly aggregate runs"),
        0,
    );
}

#[test]
fn generated_correlation_ids_are_distinct_and_nonempty() {
    let first = CorrelationId::generate();
    let second = CorrelationId::generate();
    assert!(!first.as_str().is_empty());
    assert_ne!(first, second);
}
