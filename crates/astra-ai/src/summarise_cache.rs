// SPDX-License-Identifier: GPL-3.0-only
//! The summarise cache store: the keyed get and put the cached
//! summarise action rides over [`AiStore`]. One row per message, keyed
//! by the message's identity pair; the body hash rides the row so a get
//! hits only while the summarised body is the body in front of the
//! reader, and a put replaces the row so a changed body summarises fresh
//! rather than serving the stale text.

use crate::store::AiStore;
use crate::AiStoreError;

/// One cached summarise row as the store reads it back: every column of
/// the `summarise_cache` table. Serialisable so the CLI can carry the row
/// across the process boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SummariseCacheRow {
    /// The account owning the message.
    pub message_account: String,
    /// The message's provider key, unique only within its account.
    pub message_key: String,
    /// The body hash the summary was cached under: the row is a serving
    /// hit only while the body in front of the reader hashes to it.
    pub body_hash: String,
    /// The cached summary text.
    pub summary: String,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

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

    /// The message's cached row, whatever hash it rides: the store-side
    /// read the CLI's summary verb answers from. `None` names a message
    /// no summarise has cached. Whether the row still hits is the
    /// caller's body-hash comparison; this read makes no claim about it,
    /// the hash-gated [`cached_summary`](Self::cached_summary) stays the
    /// serving path's own.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn summarise_cache_for_message(
        &self,
        account: &str,
        key: &str,
    ) -> Result<Option<SummariseCacheRow>, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT message_account, message_key, body_hash, summary, created_at
             FROM summarise_cache
             WHERE message_account = ?1 AND message_key = ?2",
            [account, key],
            |row| {
                Ok(SummariseCacheRow {
                    message_account: row.get(0)?,
                    message_key: row.get(1)?,
                    body_hash: row.get(2)?,
                    summary: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
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
