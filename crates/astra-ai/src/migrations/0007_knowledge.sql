-- The knowledge entries: one row per extracted fact, preference,
-- project, contact, or persona note (migration 0007). The kind set is
-- the extraction prompt's closed vocabulary, kept as text here because
-- the closed set is the app layer's, the `stage` convention of the AI
-- results table. The source pair records the message the entry came
-- from and the correlation id the extraction turn, so every
-- model-derived row stays traceable to its call. The unique index is
-- the exact-text dedup guard: an account never holds the same text
-- twice, so a repeated extraction refuses loudly at insert instead of
-- stacking entries; another account may hold the same text as its own.
CREATE TABLE IF NOT EXISTS knowledge (
    id TEXT PRIMARY KEY,
    account TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT NOT NULL,
    source_message_account TEXT NOT NULL,
    source_message_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_account_text
    ON knowledge(account, text);
CREATE INDEX IF NOT EXISTS idx_knowledge_account
    ON knowledge(account, created_at, id);
