-- The text cache: one row per cached derived text, keyed by the
-- message's identity pair, the kind of derivation, and the target
-- language when the derivation has one (migration 0009). The kind set
-- is the app layer's; the store round trips it as text. The body hash
-- rides the row so a get hits only while the derived-from body is the
-- body in front of the reader, the summarise cache convention; a put
-- replaces the row for its key. lang is NULL for kinds with no
-- language of their own and rows key on it in full: a langless row and
-- lang-carrying rows of the same message never collide. SQLite reads
-- NULLs as distinct in a rowid table's primary key, so the store's put
-- replaces rows by its own delete plus insert inside one transaction;
-- the declared key names the contract.
CREATE TABLE IF NOT EXISTS text_cache (
    message_account TEXT NOT NULL,
    message_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    lang TEXT,
    body_hash TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (message_account, message_key, kind, lang)
);
