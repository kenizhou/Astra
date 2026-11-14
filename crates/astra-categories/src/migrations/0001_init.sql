-- The category store: shell-owned local colour categories. Two tables:
--
--   categories        the definitions (id / name / color). Global, not
--                     per-account: a category is a user-level concept the
--                     assignment rows reference by id.
--   thread_categories one row per (account, thread, category) assignment.
--
-- Local-only: no engine write, no sync, no protocol concept. EAS and IMAP
-- accounts are served identically because the tables are protocol-agnostic
-- shell state.
--
-- The FOREIGN KEY ... ON DELETE CASCADE is the dangling-cleanup guarantee:
-- deleting a definition reclaims its assignments inside the same statement,
-- so a delete can never leave rows pointing at a gone category. The pragma
-- that enables it (rusqlite leaves foreign keys off by default) is set on
-- every open in the store.
CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS thread_categories (
    account_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    category_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id, category_id),
    FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_thread_categories_category
    ON thread_categories(category_id);