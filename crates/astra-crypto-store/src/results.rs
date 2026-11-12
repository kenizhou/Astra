// SPDX-License-Identifier: GPL-3.0-only
//! The read side of `message_crypto_results`: the per-message crypto verdict
//! the receive orchestrator writes. One row per `(account_id, message_id)`,
//! upserted on every receive, so a reader always sees the latest verdict for
//! the message it names.

use crate::CryptoStore;

/// One message's persisted crypto verdict: the framing kind, the decrypt
/// state (`ok` / `no-key` / `failed`), the signature state (the six-state
/// vocabulary's column spelling), and the resolved signer when one was
/// identified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageCryptoResult {
    /// The framing kind: `encrypted` or `encrypted-signed`.
    pub crypto_kind: String,
    /// Where the decrypt landed: `ok`, `no-key`, or `failed`.
    pub decrypt_state: String,
    /// The signature verdict's column spelling, e.g. `valid-verified`.
    pub signature_state: String,
    /// The signer's primary fingerprint, when one was identified.
    pub signer_fingerprint: Option<String>,
    /// The signer's email, when one was identified.
    pub signer_email: Option<String>,
}

/// The verdict row for `(account_id, message_id)`, or `None` when no receive
/// has landed a row for the pair (a message the crypto layer never
/// recognised writes none).
#[must_use]
pub fn get_message_crypto_result(
    store: &CryptoStore,
    account_id: &str,
    message_id: &str,
) -> Option<MessageCryptoResult> {
    store
        .conn()
        .query_row(
            "SELECT crypto_kind, decrypt_state, signature_state, signer_fingerprint, signer_email
             FROM message_crypto_results WHERE account_id = ?1 AND message_id = ?2",
            rusqlite::params![account_id, message_id],
            |row| {
                Ok(MessageCryptoResult {
                    crypto_kind: row.get(0)?,
                    decrypt_state: row.get(1)?,
                    signature_state: row.get(2)?,
                    signer_fingerprint: row.get(3)?,
                    signer_email: row.get(4)?,
                })
            },
        )
        .ok()
}
