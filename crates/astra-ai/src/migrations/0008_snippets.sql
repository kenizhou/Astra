-- The snippet library: one row per reusable text block the writer
-- keeps (migration 0008). Titles and bodies are the writer's own, never
-- model-derived; created_at is the store's stamp and updated_at is the
-- touch the update path keeps fresh, the recency the picker's
-- recent-first listing orders by. The snippet ids are caller-minted v4
-- UUIDs, the rules spelling.
CREATE TABLE IF NOT EXISTS snippets (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
