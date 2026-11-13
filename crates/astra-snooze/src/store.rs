// SPDX-License-Identifier: GPL-3.0-only
//! The sqlite-backed snooze store: one row per snoozed message, the stable
//! `Message-ID` identity beside the diagnostic provider key, the wake
//! instant, and the origin folder. The shape mirrors the AI ledger store:
//! one connection under a mutex, embedded migrations claimed in a
//! transaction, the five-second busy timeout on a file open.

use std::path::Path;
use std::sync::Mutex;

use engine_api::{AccountId, MessageIdHeader, ProviderKey};
use serde::{Deserialize, Serialize};

/// One snoozed message to record: the stable identity and the schedule. The
/// store stamps `created_at` itself. The `provider_key` is the key at snooze
/// time, diagnostic only: it goes stale on the move into the Snooze folder,
/// so the wake re-resolves by `message_id`, never by this key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSnooze {
    /// The account that owns the message.
    pub account: AccountId,
    /// The RFC 5322 `Message-ID` header, the stable identity the wake
    /// re-resolves by.
    pub message_id: MessageIdHeader,
    /// The provider key at snooze time, diagnostic only.
    pub provider_key: ProviderKey,
    /// The wake instant, unix seconds.
    pub wake_at: i64,
    /// The origin folder's key, the folder the message returns to.
    pub origin_folder: String,
    /// The subject, the re-resolution's fallback beside the sender.
    pub subject: String,
    /// The sender's address, the re-resolution's fallback beside the subject.
    pub sender: String,
}

/// One snoozed message as the store reads it back: every field of the
/// `snoozes` table, `created_at` set by the store itself at insert.
/// Serialisable so a host can carry rows across the process boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozeEntry {
    /// The store's row id, the handle a removal names.
    pub id: i64,
    /// The account that owns the message.
    pub account: AccountId,
    /// The RFC 5322 `Message-ID` header, the stable identity.
    pub message_id: MessageIdHeader,
    /// The provider key at snooze time, diagnostic only.
    pub provider_key: ProviderKey,
    /// The wake instant, unix seconds.
    pub wake_at: i64,
    /// The origin folder's key.
    pub origin_folder: String,
    /// The subject.
    pub subject: String,
    /// The sender's address.
    pub sender: String,
    /// The store's creation stamp, unix seconds.
    pub created_at: i64,
}

/// The sqlite-backed snooze store, opened by the host against the data
/// directory. The host timer reads the due rows and the core records and
/// removes them; nothing here moves mail (the core's two moves do).
#[derive(Debug)]
pub struct SnoozeStore {
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

impl SnoozeStore {
    /// Opens (creating when absent) the store at `path` and brings it to the
    /// latest embedded migration. A file store waits up to five seconds on a
    /// busy database.
    ///
    /// # Errors
    /// Fails when the database cannot be opened, when a migration fails, or
    /// when a write fails.
    pub fn open(path: &Path) -> Result<Self, SnoozeError> {
        let mut conn = rusqlite::Connection::open(path).map_err(SnoozeError::Sqlite)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(SnoozeError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An in-memory store for tests and ephemeral runs.
    ///
    /// # Errors
    /// Fails when the database cannot be opened or a migration fails.
    pub fn open_in_memory() -> Result<Self, SnoozeError> {
        let mut conn = rusqlite::Connection::open_in_memory().map_err(SnoozeError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Records one snoozed message and answers the row as it landed, the
    /// store's `created_at` stamp included.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn record(&self, row: &NewSnooze) -> Result<SnoozeEntry, SnoozeError> {
        let conn = self.lock();
        conn.query_row(
            "INSERT INTO snoozes (
                 created_at, account_id, message_id, provider_key, wake_at,
                 origin_folder, subject, sender
             ) VALUES (
                 strftime('%s','now'), ?1, ?2, ?3, ?4, ?5, ?6, ?7
             )
             RETURNING id, created_at, account_id, message_id, provider_key,
                 wake_at, origin_folder, subject, sender",
            rusqlite::params![
                row.account.as_str(),
                row.message_id.as_str(),
                row.provider_key.as_str(),
                row.wake_at,
                row.origin_folder,
                row.subject,
                row.sender,
            ],
            snooze_row,
        )
        .map_err(SnoozeError::Sqlite)?
        .ok_or(SnoozeError::Malformed)
    }

    /// The entries whose wake instant has reached `now` (`wake_at <= now`),
    /// earliest wake first. A row whose identity does not parse is skipped,
    /// not fatal: a malformed row never strands the due batch.
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn due(&self, now: i64) -> Result<Vec<SnoozeEntry>, SnoozeError> {
        self.select(
            "WHERE wake_at <= ?1 ORDER BY wake_at, id",
            &[&now as &dyn rusqlite::types::ToSql],
        )
    }

    /// Every entry, earliest wake first: the listing a future Snoozed view
    /// reads. A malformed row is skipped as in [`SnoozeStore::due`].
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn list(&self) -> Result<Vec<SnoozeEntry>, SnoozeError> {
        self.select("ORDER BY wake_at, id", &[])
    }

    /// Removes one entry by its row id: the wake's last step once the
    /// message is back in its origin folder.
    ///
    /// # Errors
    /// Fails when the delete cannot run.
    pub fn remove(&self, id: i64) -> Result<(), SnoozeError> {
        let conn = self.lock();
        conn.execute("DELETE FROM snoozes WHERE id = ?1", [id])
            .map_err(SnoozeError::Sqlite)?;
        Ok(())
    }

    /// Runs one selection over the table, skipping any row whose identity
    /// does not parse.
    fn select(
        &self,
        tail: &str,
        params: &[&dyn rusqlite::types::ToSql],
    ) -> Result<Vec<SnoozeEntry>, SnoozeError> {
        let conn = self.lock();
        let sql = format!(
            "SELECT id, created_at, account_id, message_id, provider_key, \
             wake_at, origin_folder, subject, sender FROM snoozes {tail}"
        );
        let mut statement = conn.prepare(&sql).map_err(SnoozeError::Sqlite)?;
        let rows = statement
            .query_map(params, snooze_row)
            .map_err(SnoozeError::Sqlite)?;
        let mut entries = Vec::new();
        for row in rows {
            if let Some(entry) = row.map_err(SnoozeError::Sqlite)? {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// Brings a fresh connection to the latest embedded migration, claiming
    /// each migration's bookkeeping row first (`INSERT OR IGNORE`): the
    /// claim that changes zero rows was won by a concurrent opener, so the
    /// migration runs only inside the winner's transaction.
    fn migrate(conn: &mut rusqlite::Connection) -> Result<(), SnoozeError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snooze_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                installed_at INTEGER NOT NULL
            )",
        )
        .map_err(|source| SnoozeError::Migration {
            migration: "snooze_migrations",
            source,
        })?;
        for migration in MIGRATIONS {
            let tx = conn
                .transaction()
                .map_err(|source| SnoozeError::Migration {
                    migration: migration.name,
                    source,
                })?;
            let claimed = tx
                .execute(
                    "INSERT OR IGNORE INTO snooze_migrations (version, name, installed_at)
                     VALUES (?1, ?2, strftime('%s','now'))",
                    rusqlite::params![migration.version, migration.name],
                )
                .map_err(|source| SnoozeError::Migration {
                    migration: migration.name,
                    source,
                })?;
            if claimed == 0 {
                continue;
            }
            tx.execute_batch(migration.sql)
                .map_err(|source| SnoozeError::Migration {
                    migration: migration.name,
                    source,
                })?;
            tx.commit().map_err(|source| SnoozeError::Migration {
                migration: migration.name,
                source,
            })?;
        }
        Ok(())
    }

    /// The store's connection. An unreachable lock is a poisoned invariant,
    /// not a caller-recoverable state.
    fn lock(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.conn.lock().expect("snooze store connection poisoned")
    }
}

#[cfg(test)]
impl SnoozeStore {
    /// The connection under the store's lock, for the round-trip assertions
    /// in these tests.
    pub(crate) fn test_conn(&self) -> &Mutex<rusqlite::Connection> {
        &self.conn
    }
}

/// One result row of the `snoozes` table as a [`SnoozeEntry`], or `None`
/// when the row's account or `Message-ID` does not parse: a malformed row is
/// skipped, never fatal to the batch reading it.
fn snooze_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<SnoozeEntry>> {
    let account: String = row.get(2)?;
    let message_id: String = row.get(3)?;
    let provider_key: String = row.get(4)?;
    let Ok(account) = AccountId::try_from(account.as_str()) else {
        return Ok(None);
    };
    let Ok(message_id) = MessageIdHeader::new(message_id) else {
        return Ok(None);
    };
    let Ok(provider_key) = ProviderKey::new(provider_key) else {
        return Ok(None);
    };
    Ok(Some(SnoozeEntry {
        id: row.get(0)?,
        created_at: row.get(1)?,
        account,
        message_id,
        provider_key,
        wake_at: row.get(5)?,
        origin_folder: row.get(6)?,
        subject: row.get(7)?,
        sender: row.get(8)?,
    }))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// Store open, migration, and write failures.
#[derive(Debug, thiserror::Error)]
pub enum SnoozeError {
    /// The database could not be opened or written.
    #[error("snooze store sql failure: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An embedded migration failed; `migration` names the one.
    #[error("snooze store migration {migration} failed: {source}")]
    Migration {
        /// The failing migration's name.
        migration: &'static str,
        /// The underlying sqlite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// A row the store read carries an identity that does not parse.
    #[error("snooze store row is malformed")]
    Malformed,
}
