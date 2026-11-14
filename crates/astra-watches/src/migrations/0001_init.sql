-- The watch-replies store: one membership row per watched conversation. The
-- watch NEVER moves mail and owns no engine state; the row is a plain mark
-- from set until the first reply clears it (the one-shot consume rides the
-- same unwatch the menu does). member_key is one representative provider key
-- of the conversation captured at set time: a derived thread is named the
-- smallest owned Message-ID in its component and re-keys on merge, so the
-- check re-resolves this key through the engine at check time rather than
-- trusting the thread_id captured at set time.
CREATE TABLE IF NOT EXISTS watch_replies (
    account_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    member_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);