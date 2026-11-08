-- The analytics events ledger: one row per consented product event,
-- tied to no correlation id (these are product events, not agent turns).
-- The counts column is a JSON object of name-to-integer pairs: the values
-- are integers by type, so no subject, address, or body can ride a value,
-- and the column shape itself is what keeps it so (a column that does not
-- exist cannot be filled by a later change that forgets the rule). The
-- consent gate lives in the app core; the store records what it is given.
CREATE TABLE IF NOT EXISTS analytics_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    shape TEXT NOT NULL,
    counts TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_analytics_events_created ON analytics_events(created_at);
CREATE INDEX IF NOT EXISTS idx_analytics_events_shape ON analytics_events(shape);
