// SPDX-License-Identifier: GPL-3.0-only
//! The AI results store: the row types and the record + query APIs the
//! incoming pass drives over [`AiStore`]. One row is one AI stage's verdict
//! on one message under one pass's correlation id; a message's rows are the
//! pass's seen tracking, so the query answers newest first for the reads
//! that want the latest word. The stage and verdict vocabularies are the
//! app layer's (the closed sets live there, pinned by its tests); the store
//! round trips them as text, the `actions` convention of the rules table.

use crate::correlation::CorrelationId;
use crate::rules::RuleId;
use crate::store::AiStore;
use crate::AiStoreError;

/// One AI result to insert: every column the caller owns; the store stamps
/// `created_at` itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAiResult {
    /// The pass that judged.
    pub correlation: CorrelationId,
    /// The account owning the judged message.
    pub message_account: String,
    /// The message's provider key, unique only within its account.
    pub message_key: String,
    /// The stage that judged: the app layer's closed set as its wire word.
    pub stage: String,
    /// The rule the stage judged, for the rule-match stage; `None` for the
    /// message-level stages.
    pub rule_id: Option<RuleId>,
    /// The outcome word: the verdict, or the safe default's name.
    pub verdict: String,
    /// The category a categorisation assigned; `None` for every other
    /// stage.
    pub category: Option<String>,
}

/// One AI result as the store reads it back: every column of the
/// `ai_results` table, `created_at` set by the store itself at insert.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AiResultRow {
    /// The pass that judged.
    pub correlation: CorrelationId,
    /// The account owning the judged message.
    pub message_account: String,
    /// The message's provider key, unique only within its account.
    pub message_key: String,
    /// The stage that judged: the app layer's closed set as its wire word.
    pub stage: String,
    /// The rule the stage judged, for the rule-match stage; `None` for the
    /// message-level stages.
    pub rule_id: Option<RuleId>,
    /// The outcome word: the verdict, or the safe default's name.
    pub verdict: String,
    /// The category a categorisation assigned; `None` for every other
    /// stage.
    pub category: Option<String>,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

impl AiStore {
    /// Appends one AI result row. As the ledger convention, the store sets
    /// `created_at` itself.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn record_ai_result(&self, row: &NewAiResult) -> Result<(), AiStoreError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO ai_results (
                 created_at, correlation_id, message_account, message_key,
                 stage, rule_id, verdict, category
             ) VALUES (
                 strftime('%s','now'), ?1, ?2, ?3, ?4, ?5, ?6, ?7
             )",
            rusqlite::params![
                row.correlation.as_str(),
                row.message_account,
                row.message_key,
                row.stage,
                row.rule_id.as_ref().map(RuleId::as_str),
                row.verdict,
                row.category,
            ],
        )
        .map_err(AiStoreError::Sqlite)?;
        Ok(())
    }

    /// Every result recorded for one message, newest first (creation time,
    /// then row id, so same-second inserts still order deterministically):
    /// the read the pass's seen check and the verdict surfaces share. An
    /// empty answer names a message no pass has judged.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn ai_results_for_message(
        &self,
        account: &str,
        key: &str,
    ) -> Result<Vec<AiResultRow>, AiStoreError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT correlation_id, message_account, message_key, stage,
                        rule_id, verdict, category, created_at
                 FROM ai_results
                 WHERE message_account = ?1 AND message_key = ?2
                 ORDER BY created_at DESC, id DESC",
            )
            .map_err(AiStoreError::Sqlite)?;
        let rows = statement
            .query_map(rusqlite::params![account, key], |row| {
                Ok(AiResultRow {
                    correlation: CorrelationId::new(row.get(0)?),
                    message_account: row.get(1)?,
                    message_key: row.get(2)?,
                    stage: row.get(3)?,
                    rule_id: row.get::<_, Option<String>>(4)?.map(RuleId::new),
                    verdict: row.get(5)?,
                    category: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(AiStoreError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(AiStoreError::Sqlite)
    }

    /// Every audit row one correlation id owns, oldest first: the read the
    /// execution contract's assertions and the CLI's audit surface share.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn audit_rows_for_correlation(
        &self,
        correlation: &CorrelationId,
    ) -> Result<Vec<crate::store::AuditRow>, AiStoreError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT correlation_id, verb, arg_preview, outcome
                 FROM audit_log WHERE correlation_id = ?1 ORDER BY id",
            )
            .map_err(AiStoreError::Sqlite)?;
        let rows = statement
            .query_map(rusqlite::params![correlation.as_str()], |row| {
                Ok(crate::store::AuditRow {
                    correlation: CorrelationId::new(row.get(0)?),
                    verb: row.get(1)?,
                    arg_preview: row.get(2)?,
                    outcome: row.get(3)?,
                })
            })
            .map_err(AiStoreError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(AiStoreError::Sqlite)
    }
}
