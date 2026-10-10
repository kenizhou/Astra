-- The chips-dismissed markers: one row per message whose smart-reply chips
-- the reader dismissed (migration 0006). A user action, not a model verdict:
-- it lives outside ai_results so the incoming pass's durable seen tracking
-- never mistakes a dismissal for a judged message, and outside the draft
-- pool because dismissal hides every chip the message may ever grow, not
-- one generation's row. Presence alone is the marker; the store stamps
-- created_at for the audit trail.
CREATE TABLE IF NOT EXISTS chips_dismissed (
    message_account TEXT NOT NULL,
    message_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (message_account, message_key)
);
