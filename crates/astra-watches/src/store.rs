// SPDX-License-Identifier: GPL-3.0-only
//! The sqlite-backed watch-replies store: one membership row per watched
//! (account, thread), the representative member key captured at set time,
//! and the creation stamp. The shape mirrors the reminder store: one
//! connection under a mutex, embedded migrations claimed in a transaction,
//! the five-second busy timeout on a file open. There is no due logic and no
//! timer: the arrival lane is the only trigger, and the one-shot consume
//! rides the same `unwatch` the menu does.

use std::path::Path;
use std::sync::Mutex;

use engine_api::{AccountId, ProviderKey};
use serde::{Deserialize, Serialize};

/// One watch to record: the membership and its representative link. The
/// store stamps `created_at` itself. The `member_key` is one provider key of
/// the watched conversation captured at set time; the check re-resolves it
/// through the engine so a derived thread's re-keying never strands the
/// watch (the stored `thread_id` is the row's handle, not the match key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWatch {
    /// The account that owns the conversation.
    pub account: AccountId,
    /// The thread id resolved at set time, the row's handle within the account.
    pub thread_id: String,
    /// One representative provider key of the conversation at set time.
    pub member_key: ProviderKey,
}

/// One watch as the store reads it back: every field of the `watch_replies`
/// table, `created_at` set by the store itself at insert. Serialisable so a
/// host can carry rows across the process boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEntry {
    /// The account that owns the conversation.
    pub account: AccountId,
    /// The thread id resolved at set time, the row's handle within the account.
    pub thread_id: String,
    /// The representative provider key captured at set time.
    pub member_key: ProviderKey,
    /// The store's creation stamp, unix seconds.
    pub created_at: i64,
}

/// The sqlite-backed watch store, opened by the host against the data
/// directory. The core records and clears the marks and the arrival check
/// reads them; nothing here moves mail (a watch never moves it).
#[derive(Debug)]
pub struct WatchStore {
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

impl WatchStore {
    /// Opens (creating when absent) the store at `path` and brings it to the
    /// latest embedded migration. A file store waits up to five seconds on a
    /// busy database.
    ///
    /// # Errors
    /// Fails when the database cannot be opened, when a migration fails, or
    /// when a write fails.
    pub fn open(path: &Path) -> Result<Self, WatchError> {
        let mut conn = rusqlite::Connection::open(path).map_err(WatchError::Sqlite)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(WatchError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An in-memory store for tests and ephemeral runs.
    ///
    /// # Errors
    /// Fails when the database cannot be opened or a migration fails.
    pub fn open_in_memory() -> Result<Self, WatchError> {
        let mut conn = rusqlite::Connection::open_in_memory().map_err(WatchError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Watches one conversation, idempotent on (account, thread): a re-watch
    /// keeps the standing row (its member link included) and answers `false`;
    /// a fresh watch lands the row and answers `true`.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn watch(&self, row: &NewWatch) -> Result<bool, WatchError> {
        let conn = self.lock();
        let inserted = conn
            .execute(
                "INSERT INTO watch_replies (created_at, account_id, thread_id, member_key)
                 VALUES (strftime('%s','now'), ?1, ?2, ?3)
                 ON CONFLICT (account_id, thread_id) DO NOTHING",
                rusqlite::params![row.account.as_str(), row.thread_id, row.member_key.as_str(),],
            )
            .map_err(WatchError::Sqlite)?;
        Ok(inserted > 0)
    }

    /// Unwatches one conversation: the menu's clear and the one-shot consume
    /// on the first reply ride this exact method. Answers whether a row was
    /// removed (idempotent).
    ///
    /// # Errors
    /// Fails when the delete cannot run.
    pub fn unwatch(&self, account: &AccountId, thread_id: &str) -> Result<bool, WatchError> {
        let conn = self.lock();
        let removed = conn
            .execute(
                "DELETE FROM watch_replies WHERE account_id = ?1 AND thread_id = ?2",
                rusqlite::params![account.as_str(), thread_id],
            )
            .map_err(WatchError::Sqlite)?;
        Ok(removed > 0)
    }

    /// Every watch, insertion (rowid) order across accounts: the deterministic
    /// listing a future settings surface reads. A row whose identity does not
    /// parse is skipped, never fatal to the listing.
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn list(&self) -> Result<Vec<WatchEntry>, WatchError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT account_id, thread_id, member_key, created_at
                 FROM watch_replies ORDER BY rowid",
            )
            .map_err(WatchError::Sqlite)?;
        let rows = statement
            .query_map([], watch_row)
            .map_err(WatchError::Sqlite)?;
        let mut entries = Vec::new();
        for row in rows {
            if let Some(entry) = row.map_err(WatchError::Sqlite)? {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// Brings a fresh connection to the latest embedded migration, claiming
    /// each migration's bookkeeping row first (`INSERT OR IGNORE`): the
    /// claim that changes zero rows was won by a concurrent opener, so the
    /// migration runs only inside the winner's transaction.
    fn migrate(conn: &mut rusqlite::Connection) -> Result<(), WatchError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS watch_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                installed_at INTEGER NOT NULL
            )",
        )
        .map_err(|source| WatchError::Migration {
            migration: "watch_migrations",
            source,
        })?;
        for migration in MIGRATIONS {
            let tx = conn.transaction().map_err(|source| WatchError::Migration {
                migration: migration.name,
                source,
            })?;
            let claimed = tx
                .execute(
                    "INSERT OR IGNORE INTO watch_migrations (version, name, installed_at)
                     VALUES (?1, ?2, strftime('%s','now'))",
                    rusqlite::params![migration.version, migration.name],
                )
                .map_err(|source| WatchError::Migration {
                    migration: migration.name,
                    source,
                })?;
            if claimed == 0 {
                continue;
            }
            tx.execute_batch(migration.sql)
                .map_err(|source| WatchError::Migration {
                    migration: migration.name,
                    source,
                })?;
            tx.commit().map_err(|source| WatchError::Migration {
                migration: migration.name,
                source,
            })?;
        }
        Ok(())
    }

    /// The store's connection. An unreachable lock is a poisoned invariant,
    /// not a caller-recoverable state.
    fn lock(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.conn.lock().expect("watch store connection poisoned")
    }
}

#[cfg(test)]
impl WatchStore {
    /// The connection under the store's lock, for the round-trip assertions
    /// in these tests.
    pub(crate) fn test_conn(&self) -> &Mutex<rusqlite::Connection> {
        &self.conn
    }
}

/// One result row of the `watch_replies` table as a [`WatchEntry`], or `None`
/// when the row's account or member key does not parse: a malformed row is
/// skipped, never fatal to the listing reading it.
fn watch_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<WatchEntry>> {
    let account: String = row.get(0)?;
    let thread_id: String = row.get(1)?;
    let member_key: String = row.get(2)?;
    let Ok(account) = AccountId::try_from(account.as_str()) else {
        return Ok(None);
    };
    let Ok(member_key) = ProviderKey::new(member_key) else {
        return Ok(None);
    };
    Ok(Some(WatchEntry {
        account,
        thread_id,
        member_key,
        created_at: row.get(3)?,
    }))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// Store open, migration, and write failures.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    /// The database could not be opened or written.
    #[error("watch store sql failure: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An embedded migration failed; `migration` names the one.
    #[error("watch store migration {migration} failed: {source}")]
    Migration {
        /// The failing migration's name.
        migration: &'static str,
        /// The underlying sqlite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// A row the store read carries an identity that does not parse.
    #[error("watch store row is malformed")]
    Malformed,
}
