-- The AI results table: one row per AI stage verdict on one message,
-- keyed by the pass's correlation id (migration 0003). The stage set is
-- rule_match | cold | categorise (the A1 stages; cold and categorise land
-- with their slices). The verdict is the outcome word: yes, no, or
-- abstain_default, the last naming the safe-default table's answer rather
-- than the model's. A message's rows are its seen tracking: the incoming
-- pass skips a message that already has any.
CREATE TABLE IF NOT EXISTS ai_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL,
    message_account TEXT NOT NULL,
    message_key TEXT NOT NULL,
    stage TEXT NOT NULL,
    rule_id TEXT,
    verdict TEXT NOT NULL,
    category TEXT
);
CREATE INDEX IF NOT EXISTS idx_ai_results_message ON ai_results(message_account, message_key);
CREATE INDEX IF NOT EXISTS idx_ai_results_correlation ON ai_results(correlation_id);
