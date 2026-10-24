// SPDX-License-Identifier: GPL-3.0-only
//! The analytics events ledger's row types and store API: one row per
//! consented product event, recording the event's shape name and its named
//! counts. Never content: the counts are a JSON object of name-to-integer
//! pairs, so a value is an integer by type and no subject, address, or
//! body can ride one, the same rule the tool-call ledger's column set
//! follows. The consent gate lives in the app core (the recorder); the
//! store records what it is given. The rows tie to no correlation id:
//! these are product events, not agent turns.

use std::collections::BTreeMap;

use crate::store::AiStore;
use crate::AiStoreError;

/// One analytics event to record: the shape name and the named counts;
/// the store stamps `created_at` itself.
#[derive(Debug, Clone)]
pub struct NewAnalyticsEvent {
    /// The event's shape name, for example `sync_pass`.
    pub shape: String,
    /// The event's named counts, for example `{"messages": 12}`. The
    /// values are integers by type, so no content can ride a value.
    pub counts: BTreeMap<String, u64>,
}

/// One analytics event as the store reads it back: every field of the
/// `analytics_events` table, `created_at` set by the store itself at
/// insert. Serialisable so a host can carry rows across the process
/// boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnalyticsEventRow {
    /// The event's shape name.
    pub shape: String,
    /// The event's named counts, the name-to-integer pairs verbatim.
    pub counts: BTreeMap<String, u64>,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

impl AiStore {
    /// Appends one analytics event row and answers the row as it landed,
    /// the store's `created_at` stamp included. As the ledger convention,
    /// the store sets the stamp itself.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn record_analytics_event(
        &self,
        row: &NewAnalyticsEvent,
    ) -> Result<AnalyticsEventRow, AiStoreError> {
        let counts = counts_json(&row.counts)?;
        let conn = self.lock();
        conn.query_row(
            "INSERT INTO analytics_events (created_at, shape, counts)
             VALUES (strftime('%s','now'), ?1, ?2)
             RETURNING created_at, shape, counts",
            rusqlite::params![row.shape, counts],
            analytics_event_row,
        )
        .map_err(AiStoreError::Sqlite)
    }

    /// The most recent analytics events, newest first, at most `limit` of
    /// them. Ordered by the table's own id (insertion order), so events
    /// landing within one second still read in the order they arrived.
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn recent_analytics_events(
        &self,
        limit: usize,
    ) -> Result<Vec<AnalyticsEventRow>, AiStoreError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT created_at, shape, counts FROM analytics_events
                 ORDER BY id DESC LIMIT ?1",
            )
            .map_err(AiStoreError::Sqlite)?;
        let rows = statement
            .query_map(
                [i64::try_from(limit).unwrap_or(i64::MAX)],
                analytics_event_row,
            )
            .map_err(AiStoreError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(AiStoreError::Sqlite)
    }
}

/// The counts as the row stores them: a JSON object of name-to-integer
/// pairs. A pair that cannot serialise is a SQL parameter that cannot
/// convert, the store's own error shape.
fn counts_json(counts: &BTreeMap<String, u64>) -> Result<String, AiStoreError> {
    serde_json::to_string(counts)
        .map_err(|err| AiStoreError::Sqlite(rusqlite::Error::ToSqlConversionFailure(err.into())))
}

/// One result row of the `analytics_events` table as an
/// [`AnalyticsEventRow`]. A counts column that does not parse is a
/// conversion failure, the store's own read error shape.
fn analytics_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnalyticsEventRow> {
    let counts: String = row.get(2)?;
    let counts = serde_json::from_str(&counts).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(err))
    })?;
    Ok(AnalyticsEventRow {
        created_at: row.get(0)?,
        shape: row.get(1)?,
        counts,
    })
}

#[cfg(test)]
#[path = "analytics_events_tests.rs"]
mod tests;
