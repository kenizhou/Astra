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
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "rules",
        sql: include_str!("migrations/0002_rules.sql"),
    },
    Migration {
        version: 3,
        name: "ai_results",
        sql: include_str!("migrations/0003_ai_results.sql"),
    },
    Migration {
        version: 4,
        name: "draft_pool",
        sql: include_str!("migrations/0004_draft_pool.sql"),
    },
    Migration {
        version: 5,
        name: "summarise_cache",
        sql: include_str!("migrations/0005_summarise_cache.sql"),
    },
    Migration {
        version: 6,
        name: "chip_dismissals",
        sql: include_str!("migrations/0006_chip_dismissals.sql"),
    },
    Migration {
        version: 7,
        name: "knowledge",
        sql: include_str!("migrations/0007_knowledge.sql"),
    },
    Migration {
        version: 8,
        name: "snippets",
        sql: include_str!("migrations/0008_snippets.sql"),
    },
    Migration {
        version: 9,
        name: "text_cache",
        sql: include_str!("migrations/0009_text_cache.sql"),
    },
    Migration {
        version: 10,
        name: "ai_documents",
        sql: include_str!("migrations/0010_ai_documents.sql"),
    },
    Migration {
        version: 11,
        name: "tool_calls",
        sql: include_str!("migrations/0011_tool_calls.sql"),
    },
    Migration {
        version: 12,
        name: "analytics_events",
        sql: include_str!("migrations/0012_analytics_events.sql"),
    },
];

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

    /// The nano tier's provider-reported spend over the trailing seven
    /// days, in micros of currency: the aggregate the judge spend guard
    /// compares against its `ASTRA_LLM_NANO_BUDGET_MICROS` budget. Only
    /// nano-tier rows count (the guard watches the cheap high-volume tier
    /// where the judges live); rows without a reported cost contribute
    /// nothing.
    ///
    /// # Errors
    /// Fails when the aggregate cannot run.
    pub fn weekly_nano_spend_micros(&self) -> Result<u64, AiStoreError> {
        let conn = self.lock();
        let spent: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost_micros), 0) FROM llm_calls
                 WHERE tier = 'nano' AND created_at >= strftime('%s','now') - 604800",
                [],
                |row| row.get(0),
            )
            .map_err(AiStoreError::Sqlite)?;
        Ok(u64::try_from(spent).unwrap_or(0))
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

    /// Brings a fresh connection to the latest embedded migration. The
    /// connection claims each migration's bookkeeping row first
    /// (`INSERT OR IGNORE`, asserted by change count): the claim that
    /// changes zero rows was won by a concurrent opener, the migration
    /// runs only inside the winner's transaction together with that row,
    /// and an interrupted migration leaves no half-applied schema. There
    /// is no check-then-act: the host and a CLI invocation opening the
    /// same fresh file interleave freely.
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
            let tx = conn
                .transaction()
                .map_err(|source| AiStoreError::Migration {
                    migration: migration.name,
                    source,
                })?;
            let claimed = tx
                .execute(
                    "INSERT OR IGNORE INTO ai_migrations (version, name, installed_at)
                     VALUES (?1, ?2, strftime('%s','now'))",
                    rusqlite::params![migration.version, migration.name],
                )
                .map_err(|source| AiStoreError::Migration {
                    migration: migration.name,
                    source,
                })?;
            if claimed == 0 {
                // A concurrent opener applied this migration first; the
                // claim wrote nothing, so dropping the transaction is a
                // no-op rollback.
                continue;
            }
            tx.execute_batch(migration.sql)
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
    /// not a caller-recoverable state. Shared with this crate's rules CRUD,
    /// which rides the same connection under the same contract.
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
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
