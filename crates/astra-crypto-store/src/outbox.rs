// SPDX-License-Identifier: GPL-3.0-only
//! The Astra-side crypto outbox: the durable queue the gated sends wait in
//! (SP4b spec D3). kylins' sends rode the engine's own queue; the pinned
//! engine takes only `Draft` payloads, so wrapped bytes queue here instead
//! and the bypass submitter drains them. The contract mirrors the engine
//! outbox's shape rather than duplicating its code: `queued`/`sending`/
//! `unconfirmed`/`failed`, the mid-DATA-loss row landing in `unconfirmed`
//! (never auto-retried), and the user's answer the only re-send path.

use serde::{Deserialize, Serialize};

use crate::store::CryptoStore;

/// One queued crypto send.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CryptoOutboxRow {
    /// Autoincrement row id.
    pub id: i64,
    /// The account the send leaves from.
    pub account_id: String,
    /// `'openpgp' | 'smime'`.
    pub standard: String,
    /// The subject line, for the outbox row's display.
    pub subject: String,
    /// The envelope's MAIL FROM address.
    pub env_from: String,
    /// The envelope's RCPT TO list (every recipient, Bcc included).
    pub env_to: Vec<String>,
    /// The wrapped over-the-wire message (no `Bcc` header).
    #[serde(skip)]
    pub wire_bytes: Vec<u8>,
    /// The wrapped filed (Sent copy) message, `Bcc` header included.
    #[serde(skip)]
    pub filed_bytes: Vec<u8>,
    /// `'queued' | 'sending' | 'unconfirmed' | 'failed'`.
    pub state: String,
    /// The last failure, for the failed row's display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Unix-seconds string, stamped at enqueue.
    pub created_at: String,
    /// Unix-seconds string of the last submit attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempted_at: Option<String>,
}

/// Enqueue a gated send. Returns the new row id.
///
/// # Errors
/// Fails when the insert cannot run.
#[allow(clippy::too_many_arguments)] // the row's full shape; bundling adds ceremony without clarity
pub fn enqueue_crypto_send(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    subject: &str,
    wire_bytes: &[u8],
    filed_bytes: &[u8],
    env_from: &str,
    env_to: &[String],
) -> Result<i64, String> {
    let conn = store.conn();
    let env_to_json = serde_json::to_string(env_to).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO crypto_outbox
         (account_id, standard, subject, env_from, env_to, wire_bytes, filed_bytes, state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', strftime('%s','now'))",
        rusqlite::params![account_id, standard, subject, env_from, env_to_json, wire_bytes, filed_bytes],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

const COLS: &str = "id, account_id, standard, subject, env_from, env_to, wire_bytes, \
     filed_bytes, state, error, created_at, attempted_at";

fn row_to_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<CryptoOutboxRow> {
    Ok(CryptoOutboxRow {
        id: row.get(0)?,
        account_id: row.get(1)?,
        standard: row.get(2)?,
        subject: row.get(3)?,
        env_from: row.get(4)?,
        env_to: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        wire_bytes: row.get(6)?,
        filed_bytes: row.get(7)?,
        state: row.get(8)?,
        error: row.get(9)?,
        created_at: row.get(10)?,
        attempted_at: row.get(11)?,
    })
}

/// List every outbox row (any state), oldest first: the sending snapshot's
/// read.
///
/// # Errors
/// Fails on a database error.
pub fn list_crypto_outbox(store: &CryptoStore) -> Result<Vec<CryptoOutboxRow>, String> {
    let conn = store.conn();
    let mut stmt = conn
        .prepare(&format!("SELECT {COLS} FROM crypto_outbox ORDER BY id"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_outbox)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// List the rows due for a submit attempt: `queued` (never tried, or a
/// transient failure being retried) plus `failed` rows requeued by send-now
/// (which lands them back in `queued`). `sending` (in-flight marker from a
/// previous pass whose process died) also drains: a dead attempt left no
/// proof of delivery, and the SMTP conversation's own idempotence is the
/// duplicate guard, so a stale `sending` row retries as `queued` would.
///
/// # Errors
/// Fails on a database error.
pub fn list_due_crypto_sends(store: &CryptoStore) -> Result<Vec<CryptoOutboxRow>, String> {
    let conn = store.conn();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM crypto_outbox WHERE state IN ('queued','sending') ORDER BY id"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_outbox)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Mark a row in-flight for the attempt now starting.
///
/// # Errors
/// Fails when the update cannot run.
pub fn mark_crypto_sending(store: &CryptoStore, id: i64) -> Result<(), String> {
    store
        .conn()
        .execute(
            "UPDATE crypto_outbox SET state = 'sending', attempted_at = strftime('%s','now') WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Land a row in `unconfirmed`: the connection died mid-DATA, so the
/// message may or may not have been delivered. Never retried automatically;
/// only the user's answer settles it.
///
/// # Errors
/// Fails when the update cannot run.
pub fn mark_crypto_unconfirmed(store: &CryptoStore, id: i64, error: &str) -> Result<(), String> {
    store
        .conn()
        .execute(
            "UPDATE crypto_outbox SET state = 'unconfirmed', error = ?2 WHERE id = ?1",
            rusqlite::params![id, error],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Mark a row permanently refused (`failed` + the reason). A send-now
/// gesture requeues it.
///
/// # Errors
/// Fails when the update cannot run.
pub fn mark_crypto_failed(store: &CryptoStore, id: i64, error: &str) -> Result<(), String> {
    store
        .conn()
        .execute(
            "UPDATE crypto_outbox SET state = 'failed', error = ?2 WHERE id = ?1",
            rusqlite::params![id, error],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Return a row to `queued` (a transient failure's backoff, or the user's
/// send-now / not-delivered answer).
///
/// # Errors
/// Fails when the update cannot run.
pub fn requeue_crypto_send(
    store: &CryptoStore,
    id: i64,
    error: Option<&str>,
) -> Result<(), String> {
    store
        .conn()
        .execute(
            "UPDATE crypto_outbox SET state = 'queued', error = ?2 WHERE id = ?1",
            rusqlite::params![id, error],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Settle a row: the send is delivered and the Sent copy filed (or the user
/// confirmed delivery, or withdrew the row). The queue's job is done; the
/// row leaves.
///
/// # Errors
/// Fails when the delete cannot run.
pub fn settle_crypto_send(store: &CryptoStore, id: i64) -> Result<(), String> {
    store
        .conn()
        .execute(
            "DELETE FROM crypto_outbox WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Read one row (the drain's per-row view).
///
/// # Errors
/// Fails on a database error.
pub fn get_crypto_outbox_row(
    store: &CryptoStore,
    id: i64,
) -> Result<Option<CryptoOutboxRow>, String> {
    store
        .conn()
        .query_row(
            &format!("SELECT {COLS} FROM crypto_outbox WHERE id = ?1"),
            rusqlite::params![id],
            row_to_outbox,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod outbox_tests;
