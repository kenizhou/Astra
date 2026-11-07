-- The assistant's chat threads (migration 0013): one row per thread, one
-- message row per turn side. The messages carry the rendered parts (text
-- runs, completed tool calls, notes) as JSON and the run metadata (model,
-- tier, finish reason, tool-call count, correlation id) as JSON: the
-- store's rows are the one source of truth for the visible log, while the
-- runtime's own session files under the agent data directory stay
-- engine-room and never load into the UI. The touch stamp is the thread
-- list's recency ordering; every message insert touches its chat, so the
-- thread a turn lands in surfaces first. Compaction and memory extraction
-- stay out (the SP2y spec's named remainder, not a silent drop).
CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_touched_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chats_touched
    ON chats(last_touched_at DESC, id DESC);
CREATE TABLE IF NOT EXISTS chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id TEXT NOT NULL,
    role TEXT NOT NULL,
    parts TEXT NOT NULL,
    metadata TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chat_messages_chat
    ON chat_messages(chat_id, id);
