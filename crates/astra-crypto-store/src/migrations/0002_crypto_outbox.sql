-- SP4b D3: the Astra-side crypto outbox. One row per gated send waiting on
-- the bypass submitter: the wrapped over-the-wire bytes (no Bcc header) and
-- the wrapped filed bytes (with it), so a Sent copy is exactly what went
-- out. State machine: queued -> sending -> (deleted on success) |
-- unconfirmed (mid-DATA loss; only the user's answer settles it) | failed
-- (permanent refusal; "send now" requeues) | queued (transient, retried).

CREATE TABLE crypto_outbox (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id   TEXT NOT NULL,
    standard     TEXT NOT NULL CHECK(standard IN ('openpgp','smime')),
    subject      TEXT NOT NULL DEFAULT '',
    env_from     TEXT NOT NULL DEFAULT '',
    env_to       TEXT NOT NULL DEFAULT '',   -- JSON array of envelope recipients (incl. Bcc)
    wire_bytes   BLOB NOT NULL,
    filed_bytes  BLOB NOT NULL,
    state        TEXT NOT NULL DEFAULT 'queued'
                  CHECK(state IN ('queued','sending','unconfirmed','failed')),
    error        TEXT,
    created_at   TEXT NOT NULL,
    attempted_at TEXT
);
CREATE INDEX idx_crypto_outbox_state ON crypto_outbox(state, account_id);
