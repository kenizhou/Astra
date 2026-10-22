// SPDX-License-Identifier: GPL-3.0-only
//! The store handle: one `crypto.sqlite`, an embedded migration chain (the
//! `astra-ai` store idiom), and the account registry the schema's foreign
//! keys need.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

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

/// The sqlite-backed crypto store. All query layers in this crate take a
/// `&CryptoStore` and lock the connection per statement, matching
/// `astra-ai`'s store discipline (rusqlite, one linked sqlite with the
/// engine).
#[derive(Debug)]
pub struct CryptoStore {
    conn: Mutex<Connection>,
}

impl CryptoStore {
    /// Opens (creating when absent) the store at `path` and brings it to the
    /// latest embedded migration. File stores wait up to five seconds on a
    /// busy database, so the host and a CLI invocation can share the file.
    ///
    /// # Errors
    /// Fails when the database cannot be opened or a migration fails.
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An in-memory store for tests and ephemeral runs.
    ///
    /// # Errors
    /// Fails when the database cannot be opened or a migration fails.
    pub fn open_in_memory() -> Result<Self, String> {
        let mut conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Locks the connection for one query batch.
    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Registers (or refreshes) the minimal account row the schema's foreign
    /// keys and the keystore bridge's email lookup need. The app calls this
    /// when a mail account is configured; it is idempotent.
    ///
    /// # Errors
    /// Fails when the upsert cannot run.
    pub fn register_account(&self, account_id: &str, email: &str) -> Result<(), String> {
        self.conn()
            .execute(
                "INSERT INTO accounts (id, email) VALUES (?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET email = excluded.email",
                rusqlite::params![account_id, email],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The account email the keystore bridge's `put` binds to the key row,
    /// or `None` when the account is not registered.
    ///
    /// # Errors
    /// Fails on a database error.
    pub(crate) fn account_email(&self, account_id: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT email FROM accounts WHERE id = ?1",
                rusqlite::params![account_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.to_string()),
            })
    }

    fn migrate(conn: &mut Connection) -> Result<(), String> {
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                 version INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 applied_at TEXT NOT NULL DEFAULT (strftime('%s','now'))
             )",
        )
        .map_err(|e| e.to_string())?;
        let applied: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        for migration in MIGRATIONS {
            if migration.version <= applied {
                continue;
            }
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            tx.execute_batch(migration.sql).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO _migrations (version, name) VALUES (?1, ?2)",
                rusqlite::params![migration.version, migration.name],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
