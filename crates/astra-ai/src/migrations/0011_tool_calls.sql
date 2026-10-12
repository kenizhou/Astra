-- The tool-call ledger: one row per MCP tools/call, tied to the call's
-- correlation id. The argument column is a CHARACTER COUNT, never the
-- arguments: no query, address, subject, or body crosses into the ledger,
-- and the column shape itself is what keeps it so (a column that does not
-- exist cannot be filled by a later change that forgets the rule).
CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    outcome TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    arg_chars INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tool_calls_created ON tool_calls(created_at);
CREATE INDEX IF NOT EXISTS idx_tool_calls_correlation ON tool_calls(correlation_id);
