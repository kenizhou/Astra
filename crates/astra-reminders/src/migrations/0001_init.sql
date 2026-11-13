-- The reminder store: one row per flag reminder. The message never moves,
-- so the provider key recorded at set time stays a valid link and the wake
-- opens the row by it directly (no Message-ID re-resolution). wake_at is
-- unix seconds; subject and sender are captured at set time so the wake
-- builds its notice without an engine read.
CREATE TABLE IF NOT EXISTS reminders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    account_id TEXT NOT NULL,
    provider_key TEXT NOT NULL,
    wake_at INTEGER NOT NULL,
    subject TEXT NOT NULL,
    sender TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_reminders_wake_at ON reminders(wake_at);
