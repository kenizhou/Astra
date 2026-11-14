// SPDX-License-Identifier: GPL-3.0-only
//! The sqlite-backed category store: global colour definitions and one
//! assignment row per (account, thread, category). A local overlay, so
//! nothing here writes the engine. The shape mirrors the reminder store: one
//! connection under a mutex, embedded migrations claimed in a transaction,
//! the five-second busy timeout on a file open. Foreign keys are enabled on
//! every open so a definition's delete cascades its assignments away.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use engine_api::AccountId;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::CategoryId;

/// The longest name accepted (trimmed); ids derive from names, menus truncate.
const MAX_NAME_LEN: usize = 64;

/// One category definition as the store reads it back: the slug id, the name,
/// and the `#RRGGBB` colour. Serialisable so a host carries defs across the
/// process boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryDef {
    /// The definition's slug id, the key the assignments reference.
    pub id: CategoryId,
    /// The display name, unique case-insensitively.
    pub name: String,
    /// The colour, a `#RRGGBB` hex spelling.
    pub color: String,
}

/// The sqlite-backed category store, opened by the host against the data
/// directory. The core reads and writes definitions and assignments; nothing
/// here moves mail or touches the engine's keywords (a category is local).
#[derive(Debug)]
pub struct CategoryStore {
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

impl CategoryStore {
    /// Opens (creating when absent) the store at `path` and brings it to the
    /// latest embedded migration, foreign keys enabled. A file store waits up
    /// to five seconds on a busy database.
    ///
    /// # Errors
    /// Fails when the database cannot be opened, when a migration fails, or
    /// when the foreign-key pragma cannot be set.
    pub fn open(path: &Path) -> Result<Self, CategoryError> {
        let mut conn = rusqlite::Connection::open(path).map_err(CategoryError::Sqlite)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(CategoryError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An in-memory store for tests and ephemeral runs, foreign keys enabled.
    ///
    /// # Errors
    /// Fails when the database cannot be opened or a migration fails.
    pub fn open_in_memory() -> Result<Self, CategoryError> {
        let mut conn = rusqlite::Connection::open_in_memory().map_err(CategoryError::Sqlite)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Creates a definition: validates the trimmed name and the colour, mints
    /// the name's slug (suffixed `-2`, `-3`, ... on collision), and inserts.
    ///
    /// # Errors
    /// A typed validation refusal, a case-insensitive name clash
    /// ([`CategoryError::AlreadyExists`]), or a store failure.
    pub fn create_category(&self, name: &str, color: &str) -> Result<CategoryDef, CategoryError> {
        let name = validate(name, color)?;
        let base = slug_for(&name);
        let conn = self.lock();
        let mut id = base.clone();
        let mut n = 1;
        loop {
            let taken: Option<String> = conn
                .query_row("SELECT name FROM categories WHERE id = ?1", [&id], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(CategoryError::Sqlite)?;
            if taken.is_none() {
                break;
            }
            n += 1;
            id = format!("{base}-{n}");
        }
        conn.execute(
            "INSERT INTO categories (id, name, color, created_at)
             VALUES (?1, ?2, ?3, strftime('%s','now'))",
            rusqlite::params![id, name, color],
        )
        .map_err(|err| name_clash(err, &name))?;
        Ok(CategoryDef {
            id: CategoryId::new(id),
            name,
            color: color.to_owned(),
        })
    }

    /// Renames and recolours a definition in place. The assignments key the
    /// id, so they survive untouched.
    ///
    /// # Errors
    /// A typed validation refusal, a name clash, an unknown id
    /// ([`CategoryError::UnknownCategory`]), or a store failure.
    pub fn update_category(
        &self,
        id: &CategoryId,
        name: &str,
        color: &str,
    ) -> Result<(), CategoryError> {
        let name = validate(name, color)?;
        let conn = self.lock();
        let changed = conn
            .execute(
                "UPDATE categories SET name = ?1, color = ?2 WHERE id = ?3",
                rusqlite::params![name, color, id.as_str()],
            )
            .map_err(|err| name_clash(err, &name))?;
        if changed == 0 {
            return Err(CategoryError::UnknownCategory(id.clone()));
        }
        Ok(())
    }

    /// Deletes a definition and its assignments atomically, answering how many
    /// assignment rows the cascade removed.
    ///
    /// # Errors
    /// An unknown id ([`CategoryError::UnknownCategory`]) or a store failure.
    pub fn delete_category(&self, id: &CategoryId) -> Result<u64, CategoryError> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(CategoryError::Sqlite)?;
        // Count the assignments inside the same transaction the delete runs in,
        // so the caller's report and the cascade agree on one snapshot.
        let removed: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM thread_categories WHERE category_id = ?1",
                [id.as_str()],
                |row| row.get(0),
            )
            .map_err(CategoryError::Sqlite)?;
        let changed = tx
            .execute("DELETE FROM categories WHERE id = ?1", [id.as_str()])
            .map_err(CategoryError::Sqlite)?;
        if changed == 0 {
            // Unknown id: dropping the transaction rolls it back (the count
            // above was a read, so nothing was touched anyway).
            return Err(CategoryError::UnknownCategory(id.clone()));
        }
        tx.commit().map_err(CategoryError::Sqlite)?;
        Ok(u64::try_from(removed).unwrap_or(0))
    }

    /// Assigns one category to one (account, thread). Idempotent: an existing
    /// assignment is left untouched. Answers `true` when this call inserted.
    ///
    /// # Errors
    /// An unknown category ([`CategoryError::UnknownCategory`]) or a store
    /// failure.
    pub fn assign_category(
        &self,
        account: &AccountId,
        thread_id: &str,
        category: &CategoryId,
    ) -> Result<bool, CategoryError> {
        let conn = self.lock();
        let known: Option<String> = conn
            .query_row(
                "SELECT id FROM categories WHERE id = ?1",
                [category.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(CategoryError::Sqlite)?;
        if known.is_none() {
            return Err(CategoryError::UnknownCategory(category.clone()));
        }
        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO thread_categories
                     (account_id, thread_id, category_id, created_at)
                 VALUES (?1, ?2, ?3, strftime('%s','now'))",
                rusqlite::params![account.as_str(), thread_id, category.as_str()],
            )
            .map_err(CategoryError::Sqlite)?;
        Ok(inserted > 0)
    }

    /// Removes one category from one (account, thread). Idempotent: answers
    /// `true` when a row was removed, `false` when it was not assigned.
    ///
    /// # Errors
    /// Fails when the delete cannot run.
    pub fn unassign_category(
        &self,
        account: &AccountId,
        thread_id: &str,
        category: &CategoryId,
    ) -> Result<bool, CategoryError> {
        let conn = self.lock();
        let deleted = conn
            .execute(
                "DELETE FROM thread_categories
                 WHERE account_id = ?1 AND thread_id = ?2 AND category_id = ?3",
                rusqlite::params![account.as_str(), thread_id, category.as_str()],
            )
            .map_err(CategoryError::Sqlite)?;
        Ok(deleted > 0)
    }

    /// Every definition, creation order (rowid): a rename or recolour never
    /// reorders.
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn list_categories(&self) -> Result<Vec<CategoryDef>, CategoryError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare("SELECT id, name, color FROM categories ORDER BY rowid")
            .map_err(CategoryError::Sqlite)?;
        let rows = statement
            .query_map([], |row| {
                Ok(CategoryDef {
                    id: CategoryId::new(row.get(0)?),
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })
            .map_err(CategoryError::Sqlite)?;
        let mut defs = Vec::new();
        for row in rows {
            defs.push(row.map_err(CategoryError::Sqlite)?);
        }
        Ok(defs)
    }

    /// The per-page overlay read: which categories each of `thread_ids` (one
    /// account) carries. Per thread the ids answer assignment order (rowid);
    /// threads with no assignment, or not on the page, are absent from the map.
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn assignments_for_threads(
        &self,
        account: &AccountId,
        thread_ids: &[String],
    ) -> Result<HashMap<String, Vec<CategoryId>>, CategoryError> {
        if thread_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.lock();
        // The page ids ride a dynamically built IN (...): the placeholder count
        // is bounded by the page and every value is bound (never interpolated),
        // so the statement is fixed-shape by construction. The account is the
        // first bound value, the page ids the rest.
        let placeholders = vec!["?"; thread_ids.len()].join(", ");
        let sql = format!(
            "SELECT thread_id, category_id FROM thread_categories \
             WHERE account_id = ? AND thread_id IN ({placeholders}) \
             ORDER BY thread_id, rowid"
        );
        let mut statement = conn.prepare(&sql).map_err(CategoryError::Sqlite)?;
        let binds = std::iter::once(account.as_str()).chain(thread_ids.iter().map(String::as_str));
        let rows = statement
            .query_map(rusqlite::params_from_iter(binds), |row| {
                let thread: String = row.get(0)?;
                let category: String = row.get(1)?;
                Ok((thread, CategoryId::new(category)))
            })
            .map_err(CategoryError::Sqlite)?;
        let mut out: HashMap<String, Vec<CategoryId>> = HashMap::new();
        for row in rows {
            let (thread, category) = row.map_err(CategoryError::Sqlite)?;
            out.entry(thread).or_default().push(category);
        }
        Ok(out)
    }

    /// Brings a fresh connection to the latest embedded migration with foreign
    /// keys enabled, claiming each migration's bookkeeping row first
    /// (`INSERT OR IGNORE`): the claim that changes zero rows was won by a
    /// concurrent opener, so the migration runs only inside the winner's
    /// transaction.
    fn migrate(conn: &mut rusqlite::Connection) -> Result<(), CategoryError> {
        // rusqlite leaves foreign keys off by default; the delete cascade
        // silently no-ops without them, so every open enables them.
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(CategoryError::Sqlite)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS category_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                installed_at INTEGER NOT NULL
            )",
        )
        .map_err(|source| CategoryError::Migration {
            migration: "category_migrations",
            source,
        })?;
        for migration in MIGRATIONS {
            let tx = conn
                .transaction()
                .map_err(|source| CategoryError::Migration {
                    migration: migration.name,
                    source,
                })?;
            let claimed = tx
                .execute(
                    "INSERT OR IGNORE INTO category_migrations (version, name, installed_at)
                     VALUES (?1, ?2, strftime('%s','now'))",
                    rusqlite::params![migration.version, migration.name],
                )
                .map_err(|source| CategoryError::Migration {
                    migration: migration.name,
                    source,
                })?;
            if claimed == 0 {
                continue;
            }
            tx.execute_batch(migration.sql)
                .map_err(|source| CategoryError::Migration {
                    migration: migration.name,
                    source,
                })?;
            tx.commit().map_err(|source| CategoryError::Migration {
                migration: migration.name,
                source,
            })?;
        }
        Ok(())
    }

    /// The store's connection. An unreachable lock is a poisoned invariant,
    /// not a caller-recoverable state.
    fn lock(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.conn
            .lock()
            .expect("category store connection poisoned")
    }
}

#[cfg(test)]
impl CategoryStore {
    /// The connection under the store's lock, for the pragma and schema
    /// assertions in these tests.
    pub(crate) fn test_conn(&self) -> &Mutex<rusqlite::Connection> {
        &self.conn
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// Whether `color` is a six-digit hex RGB spelling (`#RRGGBB`): the palette the
/// manager offers emits exactly this shape, and the row colour bar and menu
/// swatch interpolate it into CSS verbatim.
fn valid_color(color: &str) -> bool {
    color
        .strip_prefix('#')
        .is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The slug id for a name: lowercase `[a-z0-9-]`, runs of anything else
/// collapsing to one dash. A symbol-only name reads as "category"; the caller
/// suffixes until unique.
fn slug_for(name: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "category".to_owned()
    } else {
        slug
    }
}

/// Validates a (name, colour) pair for create and update, answering the trimmed
/// name. Typed refusals so the host surfaces a readable message, never a raw
/// constraint string.
fn validate(name: &str, color: &str) -> Result<String, CategoryError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CategoryError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(CategoryError::NameTooLong);
    }
    if !valid_color(color) {
        return Err(CategoryError::InvalidColor(color.to_owned()));
    }
    Ok(name.to_owned())
}

/// Maps a write failure: a constraint violation is the case-insensitive name
/// clash (the id is probed unique before insert), anything else a store failure.
fn name_clash(err: rusqlite::Error, name: &str) -> CategoryError {
    match err {
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            CategoryError::AlreadyExists(name.to_owned())
        }
        other => CategoryError::Sqlite(other),
    }
}

/// Store open, migration, write, and typed validation failures.
#[derive(Debug, thiserror::Error)]
pub enum CategoryError {
    /// The database could not be opened or written.
    #[error("category store sql failure: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An embedded migration failed; `migration` names the one.
    #[error("category store migration {migration} failed: {source}")]
    Migration {
        /// The failing migration's name.
        migration: &'static str,
        /// The underlying sqlite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// The name is empty after trimming.
    #[error("category name must not be empty")]
    EmptyName,
    /// The name is longer than 64 characters.
    #[error("category name longer than 64 characters")]
    NameTooLong,
    /// The colour is not a `#RRGGBB` hex spelling.
    #[error("category colour {0:?} is not #RRGGBB hex")]
    InvalidColor(String),
    /// A definition with this name already exists (case-insensitively).
    #[error("category name {0:?} already exists")]
    AlreadyExists(String),
    /// No definition carries this id.
    #[error("unknown category {0}")]
    UnknownCategory(CategoryId),
}
