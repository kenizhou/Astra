// SPDX-License-Identifier: GPL-3.0-only
//! The chips-dismissed store: the marker the surfacing gate consults
//! before it spends a judge call. One row per message the reader
//! dismissed the smart-reply chips for (migration 0006); presence alone
//! hides the message's chips, and the row carries nothing else to grow
//! stale. A user action lives here, never in the ai_results table: those
//! rows are the incoming pass's verdicts and its durable seen tracking,
//! and a dismissal is neither.

use crate::store::AiStore;
use crate::AiStoreError;

impl AiStore {
    /// Whether the reader dismissed the message's chips: the surfacing
    /// gate's first read, ahead of any judge call.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn chips_dismissed(&self, account: &str, key: &str) -> Result<bool, AiStoreError> {
        let conn = self.lock();
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chips_dismissed
                 WHERE message_account = ?1 AND message_key = ?2",
                [account, key],
                |row| row.get(0),
            )
            .map_err(AiStoreError::Sqlite)?;
        Ok(found > 0)
    }

    /// Records the reader's dismissal of the message's chips, idempotent
    /// by the message identity pair: repeating the click keeps the one
    /// marker. As the ledger convention, the store stamps `created_at`
    /// itself.
    ///
    /// # Errors
    /// Fails when the write cannot run.
    pub fn dismiss_chips(&self, account: &str, key: &str) -> Result<(), AiStoreError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO chips_dismissed (message_account, message_key, created_at)
             VALUES (?1, ?2, strftime('%s','now'))
             ON CONFLICT(message_account, message_key) DO NOTHING",
            rusqlite::params![account, key],
        )
        .map_err(AiStoreError::Sqlite)?;
        Ok(())
    }
}
