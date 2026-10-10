-- The draft pool: one row per reply generation attempted for one
-- message, keyed by the generation's correlation id (migration 0004).
-- A drafted row carries the reply text and its confidence; a skipped
-- row (a failed or unparseable generation) carries neither, recording
-- that the message was tried without guessing a reply. Confidence is
-- the closed set's fixed numeric mapping, stored REAL so the surfacing
-- gate's threshold compares numbers. A message's rows are its
-- generation tracking: the open-message arming dedupes by pool
-- presence, skipped rows included.
CREATE TABLE IF NOT EXISTS draft_pool (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL,
    message_account TEXT NOT NULL,
    message_key TEXT NOT NULL,
    reply TEXT,
    confidence REAL
);
CREATE INDEX IF NOT EXISTS idx_draft_pool_message ON draft_pool(message_account, message_key);
-- The style profiles: the draft pool migration's sibling table, one row
-- per account. The note is the StyleInfer completion's short style
-- description; built_at is the store's stamp, the once-per-day rebuild
-- cap's clock. No Sent mail, no row: the draft prompt omits the note.
CREATE TABLE IF NOT EXISTS style_profiles (
    account TEXT PRIMARY KEY,
    note TEXT NOT NULL,
    built_at INTEGER NOT NULL
);
