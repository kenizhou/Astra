-- The AI documents: one row per stored digest or analytics report an
-- account's period holds (migration 0010). The kind is the closed set
-- digest | report, the document kinds of the A4 design; the store round
-- trips it as the snake_case wire word. The period pair bounds the
-- window the document was built over (unix seconds). The unique index
-- keys (account, kind, period_start, period_end): a regeneration of the
-- same period replaces the row, so an account holds one document per
-- kind per period, while a changed window lands as its own row beside
-- the old one's; another account holds the same period as its own. The
-- correlation id records the generation turn the document rode, so
-- every model-derived row stays traceable to its call.
CREATE TABLE IF NOT EXISTS ai_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account TEXT NOT NULL,
    kind TEXT NOT NULL,
    period_start INTEGER NOT NULL,
    period_end INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_ai_documents_period
    ON ai_documents(account, kind, period_start, period_end);
CREATE INDEX IF NOT EXISTS idx_ai_documents_account
    ON ai_documents(account, created_at, id);
