-- SP4c-4 D1: the encrypted draft mirror. The engine persists its own draft
-- copies as plaintext and cannot be changed, so Astra holds a durable
-- encrypted copy here: the body is the hex produced by vault::encrypt_with_aad
-- (AES-256-GCM under the OS-keyring master key, AAD-bound to account + draft
-- key), keyed by the draft's stable Message-ID. Resume prefers this copy; the
-- engine's own residue is a recorded gap, not purged here.

CREATE TABLE encrypted_drafts (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    draft_key  TEXT NOT NULL,
    body_enc   BLOB NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (account_id, draft_key)
);
CREATE INDEX idx_encrypted_drafts_account ON encrypted_drafts(account_id);
