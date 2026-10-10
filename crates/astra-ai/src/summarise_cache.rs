// SPDX-License-Identifier: GPL-3.0-only
//! The summarise cache store: the keyed get and put the cached
//! summarise action rides over [`AiStore`]. One row per message, keyed
//! by the message's identity pair; the body hash rides the row so a get
//! hits only while the summarised body is the body in front of the
//! reader, and a put replaces the row so a changed body summarises fresh
//! rather than serving the stale text.

use crate::store::AiStore;
use crate::AiStoreError;

impl AiStore {
    /// The cached summary for one message, when the store holds a row
    /// whose `body_hash` matches the body's current hash. Any mismatch
    /// (a changed or refetched body) answers `None`: the caller
    /// summarises fresh.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn cached_summary(
        &self,
        account: &str,
        key: &str,
        body_hash: &str,
    ) -> Result<Option<String>, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT summary FROM summarise_cache
             WHERE message_account = ?1 AND message_key = ?2 AND body_hash = ?3",
            [account, key, body_hash],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(AiStoreError::Sqlite(other)),
        })
    }

    /// Caches one summary for one message under `body_hash`, replacing
    /// any row the message already holds: the newest hash's summary is
    /// the only one the message answers. As the ledger convention, the
    /// store stamps `created_at` itself.
    ///
    /// # Errors
    /// Fails when the write cannot run.
    pub fn cache_summary(
        &self,
        account: &str,
        key: &str,
        body_hash: &str,
        summary: &str,
    ) -> Result<(), AiStoreError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO summarise_cache (
                 message_account, message_key, body_hash, summary, created_at
             ) VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))
             ON CONFLICT(message_account, message_key) DO UPDATE SET
                 body_hash = excluded.body_hash,
                 summary = excluded.summary,
                 created_at = excluded.created_at",
            rusqlite::params![account, key, body_hash, summary],
        )
        .map_err(AiStoreError::Sqlite)?;
        Ok(())
    }
}
