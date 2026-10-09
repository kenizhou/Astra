-- The AI ledgers: one row per LLM attempt (llm_calls) and one row per
-- audited agent action (audit_log), tied together by the correlation id
-- (the traceability contract). Column shapes ported from the kylins
-- ledgers, trimmed to the SPA0 fields.
CREATE TABLE IF NOT EXISTS llm_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL,
    use_case TEXT NOT NULL,
    tier TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL,
    completion_tokens INTEGER NOT NULL,
    cost_micros INTEGER,
    latency_ms INTEGER NOT NULL,
    outcome TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_llm_calls_created ON llm_calls(created_at);
CREATE INDEX IF NOT EXISTS idx_llm_calls_correlation ON llm_calls(correlation_id);
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL,
    verb TEXT NOT NULL,
    arg_preview TEXT NOT NULL,
    outcome TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_correlation ON audit_log(correlation_id);
