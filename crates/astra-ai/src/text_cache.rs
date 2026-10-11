// SPDX-License-Identifier: GPL-3.0-only
//! The text cache store: the keyed get and put the cached derived-text
//! actions ride over [`AiStore`], the summarise cache's convention
//! grown a key. One row per message, derivation kind, and language (the
//! lang column NULL for kinds with no language of their own); the body
//! hash rides the row so a get hits only while the derived-from body is
//! the body in front of the reader, and a put replaces the row for its
//! key so a changed body derives fresh rather than serving the stale
//! text. SQLite reads NULLs as distinct in a rowid table's primary key,
//! so the put replaces rows with its own delete plus insert inside one
//! transaction: a repeated langless put cannot stack a second row, and
//! a concurrent put from another process cannot interleave one.

use crate::store::AiStore;
use crate::AiStoreError;

impl AiStore {
    /// The cached text for one message's derivation, when the store
    /// holds a row for the (kind, lang) key whose `body_hash` matches
    /// the body's current hash. Any mismatch (a changed or refetched
    /// body, another kind, another lang) answers `None`: the caller
    /// derives fresh. `lang` is `None` for the langless row of the
    /// kind, a key apart from every lang-set row.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn cached_text(
        &self,
        account: &str,
        key: &str,
        kind: &str,
        lang: Option<&str>,
        body_hash: &str,
    ) -> Result<Option<String>, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT content FROM text_cache
             WHERE message_account = ?1 AND message_key = ?2 AND kind = ?3
               AND lang IS ?4 AND body_hash = ?5",
            rusqlite::params![account, key, kind, lang, body_hash],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(AiStoreError::Sqlite(other)),
        })
    }

    /// Caches one derived text for one message under its (kind, lang)
    /// key, replacing any row the key already holds: the newest put's
    /// content is the only one the key answers. The replace runs as a
    /// delete plus insert inside one transaction, the NULL-lang rows
    /// the primary key alone cannot replace. As the ledger convention,
    /// the store stamps `created_at` itself.
    ///
    /// # Errors
    /// Fails when the write cannot run.
    pub fn cache_text(
        &self,
        account: &str,
        key: &str,
        kind: &str,
        lang: Option<&str>,
        body_hash: &str,
        content: &str,
    ) -> Result<(), AiStoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(AiStoreError::Sqlite)?;
        tx.execute(
            "DELETE FROM text_cache
             WHERE message_account = ?1 AND message_key = ?2
               AND kind = ?3 AND lang IS ?4",
            rusqlite::params![account, key, kind, lang],
        )
        .map_err(AiStoreError::Sqlite)?;
        tx.execute(
            "INSERT INTO text_cache (
                 message_account, message_key, kind, lang, body_hash,
                 content, created_at
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, strftime('%s','now')
             )",
            rusqlite::params![account, key, kind, lang, body_hash, content],
        )
        .map_err(AiStoreError::Sqlite)?;
        tx.commit().map_err(AiStoreError::Sqlite)
    }
}
