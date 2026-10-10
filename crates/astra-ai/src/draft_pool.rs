// SPDX-License-Identifier: GPL-3.0-only
//! The draft pool and style profile store: the row types and the insert,
//! query, and profile APIs the reply drafting paths drive over
//! [`AiStore`]. One pool row is one reply generation attempted for one
//! message under one correlation id: a drafted row carries the reply and
//! its confidence, a skipped row (a failed or unparseable generation)
//! carries neither, and a message's rows are the generation tracking the
//! open-message arming dedupes by. The style profile is the draft pool
//! migration's sibling table: one row per account, the note beside the
//! store-stamped build time the daily rebuild cap reads.

use crate::correlation::CorrelationId;
use crate::store::AiStore;
use crate::AiStoreError;

/// One draft pool row to insert: every column the caller owns; the store
/// stamps `created_at` itself.
#[derive(Debug, Clone)]
pub struct NewDraftPoolRow {
    /// The generation turn the draft rode.
    pub correlation: CorrelationId,
    /// The account owning the message.
    pub message_account: String,
    /// The message's provider key, unique only within its account.
    pub message_key: String,
    /// The drafted reply; `None` on a skipped row.
    pub reply: Option<String>,
    /// The confidence score, the closed set's fixed numeric mapping;
    /// `None` on a skipped row.
    pub confidence: Option<f64>,
}

/// One draft pool row as the store reads it back: every column of the
/// `draft_pool` table, `created_at` set by the store itself at insert.
/// Serialisable so the CLI can carry rows across the process boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DraftPoolRow {
    /// The generation turn the draft rode.
    pub correlation: CorrelationId,
    /// The account owning the message.
    pub message_account: String,
    /// The message's provider key, unique only within its account.
    pub message_key: String,
    /// The drafted reply; `None` on a skipped row.
    pub reply: Option<String>,
    /// The confidence score; `None` on a skipped row.
    pub confidence: Option<f64>,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

/// One style profile as the store reads it back: the account's note and
/// the store-stamped build time.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StyleProfileRow {
    /// The account the profile describes.
    pub account: String,
    /// The StyleInfer completion's short style description.
    pub note: String,
    /// The store's build stamp (unix seconds), the daily rebuild cap's
    /// clock.
    pub built_at: i64,
}

impl AiStore {
    /// Appends one draft pool row and answers the row as it landed, the
    /// store's `created_at` stamp included. As the ledger convention,
    /// the store sets the stamp itself.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn insert_draft_pool(&self, row: &NewDraftPoolRow) -> Result<DraftPoolRow, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "INSERT INTO draft_pool (
                 created_at, correlation_id, message_account, message_key, reply, confidence
             ) VALUES (
                 strftime('%s','now'), ?1, ?2, ?3, ?4, ?5
             )
             RETURNING created_at, correlation_id, message_account, message_key,
                       reply, confidence",
            rusqlite::params![
                row.correlation.as_str(),
                row.message_account,
                row.message_key,
                row.reply,
                row.confidence,
            ],
            |row| {
                Ok(DraftPoolRow {
                    created_at: row.get(0)?,
                    correlation: CorrelationId::new(row.get(1)?),
                    message_account: row.get(2)?,
                    message_key: row.get(3)?,
                    reply: row.get(4)?,
                    confidence: row.get(5)?,
                })
            },
        )
        .map_err(AiStoreError::Sqlite)
    }

    /// Every pool row recorded for one message, newest first (creation
    /// time, then row id, so same-second inserts still order
    /// deterministically): the read the surfacing gate's candidates and
    /// the CLI's drafts listing share, and the open-message arming's
    /// dedup-by-pool-presence check beside them. An empty answer names a
    /// message no generation has touched, skipped rows included.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn draft_pool_for_message(
        &self,
        account: &str,
        key: &str,
    ) -> Result<Vec<DraftPoolRow>, AiStoreError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT correlation_id, message_account, message_key, reply,
                        confidence, created_at
                 FROM draft_pool
                 WHERE message_account = ?1 AND message_key = ?2
                 ORDER BY created_at DESC, id DESC",
            )
            .map_err(AiStoreError::Sqlite)?;
        let rows = statement
            .query_map(rusqlite::params![account, key], |row| {
                Ok(DraftPoolRow {
                    correlation: CorrelationId::new(row.get(0)?),
                    message_account: row.get(1)?,
                    message_key: row.get(2)?,
                    reply: row.get(3)?,
                    confidence: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(AiStoreError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(AiStoreError::Sqlite)
    }

    /// The account's style profile, when one was built. `None` is the
    /// no-profile state: no Sent mail was ever inferred from, and the
    /// draft prompt omits the note.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn style_profile(&self, account: &str) -> Result<Option<StyleProfileRow>, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT account, note, built_at FROM style_profiles WHERE account = ?1",
            [account],
            |row| {
                Ok(StyleProfileRow {
                    account: row.get(0)?,
                    note: row.get(1)?,
                    built_at: row.get(2)?,
                })
            },
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(AiStoreError::Sqlite(other)),
        })
    }

    /// Saves the account's style profile, replacing any earlier row: the
    /// newest build wins, and the store stamps `built_at` itself.
    ///
    /// # Errors
    /// Fails when the write cannot run.
    pub fn save_style_profile(&self, account: &str, note: &str) -> Result<(), AiStoreError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO style_profiles (account, note, built_at)
             VALUES (?1, ?2, strftime('%s','now'))
             ON CONFLICT(account) DO UPDATE SET
                 note = excluded.note,
                 built_at = excluded.built_at",
            rusqlite::params![account, note],
        )
        .map_err(AiStoreError::Sqlite)?;
        Ok(())
    }
}
