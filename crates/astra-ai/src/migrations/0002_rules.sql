-- The rules table: one row per AI rule. The condition is the natural
-- language sentence the generation prompt produces; the actions column
-- holds the closed action set as a JSON array (mark_read, set_flagged),
-- kept as text here because the closed set's vocabulary is the app
-- layer's: the store round trips the payload, the app parses it loudly.
CREATE TABLE IF NOT EXISTS rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    condition TEXT NOT NULL,
    actions TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
