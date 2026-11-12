-- SP4c-2 D3: extend message_crypto_results for the S/MIME receive verdict.
-- SQLite cannot ALTER a CHECK constraint, so the revocation_state CHECK gains
-- 'stale' through a table rebuild. The rebuild also adds the additive columns
-- the S/MIME chain verdict rides: the `standard` discriminator, the granular
-- `failure_reason`, and the structured `revocation_reason`. Existing rows are
-- copied verbatim (the new columns stay NULL for them), so the migration is
-- backward-compatible.

CREATE TABLE message_crypto_results_new (
    account_id         TEXT NOT NULL,
    message_id         TEXT NOT NULL,
    crypto_kind        TEXT NOT NULL CHECK(crypto_kind IN ('encrypted','signed','encrypted-signed')),
    decrypt_state      TEXT NOT NULL CHECK(decrypt_state IN ('ok','no-key','failed','n/a')),
    signature_state    TEXT NOT NULL CHECK(signature_state IN
                          ('not-signed','valid-verified','valid-unverified','invalid','unknown-key','mismatch')),
    signer_fingerprint TEXT,
    signer_email       TEXT,
    chain_valid        INTEGER,
    revocation_state   TEXT NOT NULL DEFAULT 'unchecked'
                        CHECK(revocation_state IN ('good','revoked','unchecked','stale')),
    standard           TEXT,
    failure_reason     TEXT,
    revocation_reason  TEXT,
    verified_at        TEXT NOT NULL,
    PRIMARY KEY (account_id, message_id)
);

INSERT INTO message_crypto_results_new
    (account_id, message_id, crypto_kind, decrypt_state, signature_state,
     signer_fingerprint, signer_email, chain_valid, revocation_state, verified_at)
SELECT account_id, message_id, crypto_kind, decrypt_state, signature_state,
       signer_fingerprint, signer_email, chain_valid, revocation_state, verified_at
FROM message_crypto_results;

DROP TABLE message_crypto_results;
ALTER TABLE message_crypto_results_new RENAME TO message_crypto_results;