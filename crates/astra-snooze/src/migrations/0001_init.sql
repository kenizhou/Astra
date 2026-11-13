-- The snooze store: one row per snoozed message. The stable identity is the
-- RFC 5322 Message-ID header (message_id); the provider key is diagnostic
-- only and goes stale on the move into the Snooze folder, so the wake
-- re-resolves by Message-ID, never by this key. wake_at is unix seconds;
-- origin_folder is the folder key the message returns to.
CREATE TABLE IF NOT EXISTS snoozes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    account_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    provider_key TEXT NOT NULL,
    wake_at INTEGER NOT NULL,
    origin_folder TEXT NOT NULL,
    subject TEXT NOT NULL,
    sender TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_snoozes_wake_at ON snoozes(wake_at);