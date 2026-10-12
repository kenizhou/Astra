// SPDX-License-Identifier: GPL-3.0-only
//! The tool-call ledger's row types and store API: one row per MCP
//! `tools/call`, recording which tool ran, how it ended, how long it took,
//! and how many characters of argument the caller sent. Never the arguments
//! themselves: a query, an address, or a body is content, and the ledger's
//! contract is counts and ids only. The MCP server writes a row for every
//! call and fails the call closed when the row cannot land.

use crate::correlation::CorrelationId;
use crate::store::AiStore;
use crate::AiStoreError;

/// One tool call to record: every column the caller owns; the store stamps
/// `created_at` itself.
#[derive(Debug, Clone)]
pub struct NewToolCall {
    /// The call's own correlation id, generated per call.
    pub correlation: CorrelationId,
    /// The tool's wire name, for example `search_messages`.
    pub tool: String,
    /// How the call ended, for example `ok` or `refused`.
    pub outcome: String,
    /// The call's wall time in milliseconds.
    pub duration_ms: u64,
    /// How many characters of argument the caller sent, counted across the
    /// arguments' string values.
    pub arg_chars: u64,
}

/// One tool call as the store reads it back: every column of the
/// `tool_calls` table, `created_at` set by the store itself at insert.
/// Serialisable so a host can carry rows across the process boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolCallRow {
    /// The call's own correlation id.
    pub correlation: CorrelationId,
    /// The tool's wire name.
    pub tool: String,
    /// How the call ended.
    pub outcome: String,
    /// The call's wall time in milliseconds.
    pub duration_ms: u64,
    /// How many characters of argument the caller sent.
    pub arg_chars: u64,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

impl AiStore {
    /// Appends one tool-call row and answers the row as it landed, the
    /// store's `created_at` stamp included. As the ledger convention, the
    /// store sets the stamp itself.
    ///
    /// # Errors
    /// Fails when the insert cannot run.
    pub fn record_tool_call(&self, row: &NewToolCall) -> Result<ToolCallRow, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "INSERT INTO tool_calls (
                 created_at, correlation_id, tool, outcome, duration_ms, arg_chars
             ) VALUES (
                 strftime('%s','now'), ?1, ?2, ?3, ?4, ?5
             )
             RETURNING created_at, correlation_id, tool, outcome, duration_ms, arg_chars",
            rusqlite::params![
                row.correlation.as_str(),
                row.tool,
                row.outcome,
                row.duration_ms,
                row.arg_chars,
            ],
            tool_call_row,
        )
        .map_err(AiStoreError::Sqlite)
    }

    /// The most recent tool calls, newest first, at most `limit` of them.
    /// Ordered by the table's own id (insertion order), so calls landing
    /// within one second still read in the order they arrived.
    ///
    /// # Errors
    /// Fails when the read cannot run.
    pub fn recent_tool_calls(&self, limit: usize) -> Result<Vec<ToolCallRow>, AiStoreError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT created_at, correlation_id, tool, outcome, duration_ms, arg_chars
                 FROM tool_calls ORDER BY id DESC LIMIT ?1",
            )
            .map_err(AiStoreError::Sqlite)?;
        let rows = statement
            .query_map([i64::try_from(limit).unwrap_or(i64::MAX)], tool_call_row)
            .map_err(AiStoreError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(AiStoreError::Sqlite)
    }
}

/// One result row of the `tool_calls` table as a [`ToolCallRow`].
fn tool_call_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolCallRow> {
    Ok(ToolCallRow {
        created_at: row.get(0)?,
        correlation: CorrelationId::new(row.get(1)?),
        tool: row.get(2)?,
        outcome: row.get(3)?,
        duration_ms: row.get(4)?,
        arg_chars: row.get(5)?,
    })
}

#[cfg(test)]
#[path = "tool_calls_tests.rs"]
mod tests;
