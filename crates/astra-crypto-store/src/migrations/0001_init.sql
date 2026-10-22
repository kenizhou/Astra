-- SP4b D1: the crypto store schema, ported from kylins' migrations
-- (20260710000001 crypto_keys, 20260712000001 crypto_receive,
-- 20260718200000 intermediate type, 20260726000001 origin harvest,
-- 20260727000001 cert summary, 20260904000002 contact_pinned_keys) and
-- folded to the final end-state, the way kylins' baseline folds its own
-- ALTERs. CHECK constraints, defaults, and indexes are byte-for-byte the
-- kylins final form; the recorded deviations:
--
-- * `accounts` is a MINIMAL table (id, email) owned by this store. kylins'
--   crypto_keys REFERENCES accounts(id); the Astra crypto store is a
--   standalone crypto.sqlite, so it carries the minimal account rows it
--   needs (registered by the app when a mail account is configured). The
--   keystore bridge's put() reads the account email from here, exactly as
--   kylins' does.
-- * `message_crypto_results` drops the FOREIGN KEY to the engine-side
--   messages table (that table lives in the engine's own store, not in
--   crypto.sqlite). Columns, CHECKs, and the primary key are verbatim; the
--   receive-side writer (SP4c) enforces the linkage in Rust.
-- * Everything referencing tables outside the crypto domain (contacts,
--   pending_operations) is absent; the contact_pinned_keys table is the
--   kylins post-P2 standalone form.

PRAGMA foreign_keys = ON;

-- Minimal account registry (see the header note). `email` feeds the
-- keystore bridge's find_by_email binding.
CREATE TABLE IF NOT EXISTS accounts (
    id    TEXT PRIMARY KEY,
    email TEXT NOT NULL
);

-- Crypto identity keys/certs. Soft private-key blobs are stored as the hex
-- produced by vault::encrypt_with_aad (AES-256-GCM under the OS-keyring
-- master key, AAD-bound). Token-backed keys leave private_data_enc NULL and
-- reference the device via token_serial/token_key_id.
CREATE TABLE crypto_keys (
    id               TEXT PRIMARY KEY,
    account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    standard         TEXT NOT NULL CHECK(standard IN ('openpgp','smime','sm')),
    key_type         TEXT NOT NULL CHECK(key_type IN ('public','private','cert','intermediate')),
    email            TEXT,
    fingerprint      TEXT NOT NULL,
    public_data      BLOB NOT NULL,            -- armored PGP / DER cert / SM2 cert
    private_data_enc BLOB,                     -- hex(0x01‖nonce‖ct) of soft private key; NULL for public-only/token
    token_serial     TEXT,
    token_key_id     TEXT,
    origin           TEXT NOT NULL CHECK(origin IN ('generated','imported','wkd','keyserver','autocrypt','contact','p12-intermediate','harvested','ldap')),
    is_default_sign    INTEGER NOT NULL DEFAULT 0,
    is_default_encrypt INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL,
    expires_at       TEXT,
    policy_json      TEXT,
    subject_cn       TEXT,
    not_before       TEXT,
    UNIQUE(account_id, standard, fingerprint)
);
CREATE INDEX idx_crypto_keys_email    ON crypto_keys(standard, email);
CREATE INDEX idx_crypto_keys_account  ON crypto_keys(account_id, standard);

-- Silent staging: keys seen via discovery but NOT yet accepted.
CREATE TABLE collected_keys (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id   TEXT,
    peer_email   TEXT,
    standard     TEXT,
    fingerprint  TEXT,
    public_data  BLOB,
    source       TEXT,
    seen_at      TEXT NOT NULL
);
CREATE INDEX idx_collected_keys_peer ON collected_keys(account_id, peer_email, standard);

-- Append-only trust/acceptance audit history (never UPDATE/DELETE).
CREATE TABLE trust_decisions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    TEXT NOT NULL,
    peer_email    TEXT NOT NULL,
    standard      TEXT NOT NULL,
    fingerprint   TEXT NOT NULL,
    decision      TEXT NOT NULL CHECK(decision IN ('rejected','undecided','unverified','verified','personal')),
    evidence_json TEXT,
    decided_at    TEXT NOT NULL
);
CREATE INDEX idx_trust_lookup ON trust_decisions(account_id, peer_email, standard, fingerprint, decided_at);

-- Per-contact pinned (preferred) keys: one row per normalised email, the
-- JSON array of {standard, fingerprint} objects.
CREATE TABLE contact_pinned_keys (
  email TEXT PRIMARY KEY,
  pinned_keys_json TEXT NOT NULL DEFAULT '[]'
);

-- CRL cache (keyed by distribution-point URL). `next_update` drives
-- prune_stale_crls(now_epoch) which deletes rows whose next_update < now.
CREATE TABLE crl_cache (
    crl_url     TEXT PRIMARY KEY,
    crl_der     BLOB NOT NULL,
    issuer_dn   TEXT,
    next_update TEXT,
    fetched_at  TEXT NOT NULL
);

-- Per-message crypto verification/decryption result (one row per message).
-- FK deviation recorded in the header.
CREATE TABLE message_crypto_results (
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
                        CHECK(revocation_state IN ('good','revoked','unchecked')),
    verified_at        TEXT NOT NULL,
    PRIMARY KEY (account_id, message_id)
);

-- Send-as aliases (the composer's From-address choices; carried with the
-- crypto store because the send gate resolves the signing identity through
-- them in a later slice).
CREATE TABLE send_as_aliases (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  display_name TEXT,
  reply_to_address TEXT,
  signature_id TEXT,
  is_primary INTEGER DEFAULT 0,
  is_default INTEGER DEFAULT 0,
  treat_as_alias INTEGER DEFAULT 1,
  verification_status TEXT DEFAULT 'accepted',
  created_at INTEGER DEFAULT (unixepoch()),
  UNIQUE(account_id, email)
);
CREATE INDEX idx_send_as_account ON send_as_aliases(account_id);
