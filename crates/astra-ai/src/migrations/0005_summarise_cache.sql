-- The summarise cache: one row per message, keyed by the message's
-- identity pair and invalidated by the body hash (migration 0005). A
-- put replaces the row for the message (the newest hash's summary
-- wins); a get hits only when the stored hash matches the body's
-- current hash, so an edited or refetched body misses and summarises
-- fresh rather than serving the stale text.
CREATE TABLE IF NOT EXISTS summarise_cache (
    message_account TEXT NOT NULL,
    message_key TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (message_account, message_key)
);
