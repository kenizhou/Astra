// SPDX-License-Identifier: GPL-3.0-only
//! The product SQLite store: the `llm_calls` and `audit_log` ledgers. This
//! is the product database's arrival. The store rides rusqlite, the
//! engine's own sqlite stack: sqlx's sqlite binding pins libsqlite3-sys
//! below 0.38 and the engine's rusqlite needs 0.38.x, and Cargo allows only
//! one linked sqlite3 per dependency graph, so rusqlite is what lets the
//! host and the engine link exactly one sqlite.

use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::correlation::CorrelationId;
use crate::tier::ModelTier;
use crate::use_case::LlmUseCase;

/// One LLM attempt as the ledger records it. Every attempt writes one row,
/// successful or not; the outcome vocabulary is the caller's (the ledger
/// keeps it as free-form text, the kylins store convention). Serialisable
/// so the host can carry sidecar-reported calls across the IPC boundary.
#[derive(Debug, Serialize, Deserialize)]
pub struct LlmCallRow {
    /// The turn this call served.
    pub correlation: CorrelationId,
    /// Why the call was made.
    pub use_case: LlmUseCase,
    /// The tier the call ran at (the effective tier, after any fallback).
    pub tier: ModelTier,
    /// The model that answered.
    pub model: String,
    /// Tokens reported for the prompt.
    pub prompt_tokens: u64,
    /// Tokens reported for the completion.
    pub completion_tokens: u64,
    /// The provider-reported cost in micros of currency per call, when the
    /// provider reported one.
    pub cost_micros: Option<u64>,
    /// The attempt's wall time in milliseconds.
    pub latency_ms: u64,
    /// How the attempt ended, for example `ok` or `error`.
    pub outcome: String,
}

/// One audited agent action (a CLI verb): what ran, a redacted preview of
/// its arguments, and how it ended, tied to the turn's correlation id.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditRow {
    /// The turn the action served.
    pub correlation: CorrelationId,
    /// The action's verb, for example `drafts save`.
    pub verb: String,
    /// The redacted argument preview.
    pub arg_preview: String,
    /// How the action ended, for example `ok` or `refused`.
    pub outcome: String,
}

/// The sqlite-backed AI ledger store, opened by the host and the CLI
/// against the same data dir. Both record methods append; nothing here
/// reads back (surfaces read through their own views in later slices).
#[derive(Debug)]
pub struct AiStore {
    conn: Mutex<rusqlite::Connection>,
}

/// One embedded migration: its version, its name, and its SQL.
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

/// The embedded migrations, in order.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "init",
    sql: include_str!("migrations/0001_init.sql"),
}];

impl AiStore {
    /// Opens (creating when absent) the store at `path` and brings it to
    /// the latest embedded migration. File stores wait up to five seconds
    /// on a busy database, so the host and a CLI invocation can write the
    /// same file.
    ///
    /// # Errors
    /// Fails when the database cannot be opened, when a migration fails,
    /// or when a write fails.
    pub fn open(path: &Path) -> Result<Self, AiStoreError> {
        let mut conn = rusqlite::Connection::open(path).map_err(AiStoreError::Sqlite)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(AiStoreError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An in-memory store for tests and ephemeral runs.
    ///
    /// # Errors
    /// Fails when the database cannot be opened or a migration fails.
    pub fn open_in_memory() -> Result<Self, AiStoreError> {
        let mut conn = rusqlite::Connection::open_in_memory().map_err(AiStoreError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Appends one LLM-call row. The store sets `created_at` itself
    /// (`strftime('%s','now')`, the kylins ledger convention).
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn record_llm_call(&self, row: &LlmCallRow) -> Result<(), AiStoreError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO llm_calls (
                 created_at, correlation_id, use_case, tier, model,
                 prompt_tokens, completion_tokens, cost_micros, latency_ms, outcome
             ) VALUES (
                 strftime('%s','now'), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
             )",
            rusqlite::params![
                row.correlation.as_str(),
                row.use_case.as_str(),
                row.tier.as_str(),
                row.model,
                row.prompt_tokens,
                row.completion_tokens,
                row.cost_micros,
                row.latency_ms,
                row.outcome,
            ],
        )
        .map_err(AiStoreError::Sqlite)?;
        Ok(())
    }

    /// Appends one audit row. As [`AiStore::record_llm_call`], the store
    /// sets `created_at` itself.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn record_audit(&self, row: &AuditRow) -> Result<(), AiStoreError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO audit_log (
                 created_at, correlation_id, verb, arg_preview, outcome
             ) VALUES (
                 strftime('%s','now'), ?1, ?2, ?3, ?4
             )",
            rusqlite::params![
                row.correlation.as_str(),
                row.verb,
                row.arg_preview,
                row.outcome,
            ],
        )
        .map_err(AiStoreError::Sqlite)?;
        Ok(())
    }

    /// Brings a fresh connection to the latest embedded migration. Each
    /// migration runs inside one transaction together with its bookkeeping
    /// row in `ai_migrations`, so an interrupted migration leaves no
    /// half-applied schema.
    fn migrate(conn: &mut rusqlite::Connection) -> Result<(), AiStoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ai_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                installed_at INTEGER NOT NULL
            )",
        )
        .map_err(|source| AiStoreError::Migration {
            migration: "ai_migrations",
            source,
        })?;
        for migration in MIGRATIONS {
            let applied: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM ai_migrations WHERE version = ?1)",
                    [migration.version],
                    |row| row.get(0),
                )
                .map_err(|source| AiStoreError::Migration {
                    migration: migration.name,
                    source,
                })?;
            if applied {
                continue;
            }
            let tx = conn
                .transaction()
                .map_err(|source| AiStoreError::Migration {
                    migration: migration.name,
                    source,
                })?;
            tx.execute_batch(migration.sql)
                .map_err(|source| AiStoreError::Migration {
                    migration: migration.name,
                    source,
                })?;
            tx.execute(
                "INSERT INTO ai_migrations (version, name, installed_at)
                 VALUES (?1, ?2, strftime('%s','now'))",
                rusqlite::params![migration.version, migration.name],
            )
            .map_err(|source| AiStoreError::Migration {
                migration: migration.name,
                source,
            })?;
            tx.commit().map_err(|source| AiStoreError::Migration {
                migration: migration.name,
                source,
            })?;
        }
        Ok(())
    }

    /// The store's connection. An unreachable lock is a poisoned invariant,
    /// not a caller-recoverable state.
    fn lock(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.conn.lock().expect("ai store connection poisoned")
    }
}

#[cfg(test)]
impl AiStore {
    /// The connection under the store's lock, for the round-trip
    /// assertions in these tests.
    pub(crate) fn test_conn(&self) -> &Mutex<rusqlite::Connection> {
        &self.conn
    }
}

/// Store open, migration, and write failures.
#[derive(Debug, thiserror::Error)]
pub enum AiStoreError {
    /// The database could not be opened or written.
    #[error("ai store sql failure: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An embedded migration failed; `migration` names the one.
    #[error("ai store migration {migration} failed: {source}")]
    Migration {
        /// The failing migration's name.
        migration: &'static str,
        /// The underlying sqlite failure.
        #[source]
        source: rusqlite::Error,
    },
}
