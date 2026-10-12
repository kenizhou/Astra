// SPDX-License-Identifier: GPL-3.0-only
//! The tool-call ledger: one row per `tools/call`, and the discipline that
//! keeps it honest. Every call lands a row with its own correlation id, the
//! tool's name, how it ended, how long it took, and how many characters of
//! argument the caller sent. Never the arguments: a query, an address, a
//! subject, or a body is content, and the ledger's contract is counts and
//! ids only.
//!
//! A ledger that cannot record a call fails that call closed: the server
//! answers an internal error rather than let a call pass unrecorded,
//! because an audit trail with holes is an audit trail nobody can trust.

use astra_ai::{AiStore, CorrelationId, NewToolCall};
use serde_json::Value;

/// How a tool call ended, as the ledger's outcome column spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The tool answered a result.
    Ok,
    /// Policy refused the call (the gate, or a mail-level refusal).
    Refused,
    /// The arguments did not match the tool's schema.
    InvalidParams,
    /// The tool name is not in the catalog.
    UnknownTool,
}

impl Outcome {
    /// The outcome's wire spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Refused => "refused",
            Self::InvalidParams => "invalid_params",
            Self::UnknownTool => "unknown_tool",
        }
    }
}

/// One tool call as the ledger records it.
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    /// The call's own correlation id, generated per call.
    pub correlation: CorrelationId,
    /// The tool's wire name.
    pub tool: String,
    /// How the call ended.
    pub outcome: Outcome,
    /// The call's wall time in milliseconds.
    pub duration_ms: u64,
    /// How many characters of argument the caller sent, counted across the
    /// arguments' string values.
    pub arg_chars: u64,
}

/// Why a row could not land. The caller fails the call closed on it.
#[derive(Debug, thiserror::Error)]
#[error("the tool-call ledger refused the row: {0}")]
pub struct LedgerClosed(String);

impl LedgerClosed {
    /// Wraps the store's own failure text.
    #[must_use]
    pub fn new(detail: String) -> Self {
        Self(detail)
    }
}

/// The audit sink every `tools/call` writes through.
pub trait ToolLedger: Send + Sync + 'static {
    /// Appends one row. Failing this fails the call it belongs to.
    ///
    /// # Errors
    ///
    /// [`LedgerClosed`] when the row cannot land.
    fn record(&self, record: &ToolCallRecord) -> Result<(), LedgerClosed>;
}

impl ToolLedger for AiStore {
    fn record(&self, record: &ToolCallRecord) -> Result<(), LedgerClosed> {
        self.record_tool_call(&NewToolCall {
            correlation: record.correlation.clone(),
            tool: record.tool.clone(),
            outcome: record.outcome.as_str().to_owned(),
            duration_ms: record.duration_ms,
            arg_chars: record.arg_chars,
        })
        .map(|_| ())
        .map_err(|err| LedgerClosed::new(err.to_string()))
    }
}

/// How many characters of argument the caller sent: the total length, in
/// characters rather than bytes, of every string value in the arguments,
/// recursing into objects and arrays. Numbers and booleans contribute
/// nothing; the measure is the words the caller wrote, not the punctuation
/// of their encoding.
#[must_use]
pub fn arg_chars(args: &Value) -> u64 {
    match args {
        Value::String(text) => text.chars().count() as u64,
        Value::Array(items) => items.iter().map(arg_chars).sum(),
        Value::Object(fields) => fields.values().map(arg_chars).sum(),
        _ => 0,
    }
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
