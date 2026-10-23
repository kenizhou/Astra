// SPDX-License-Identifier: GPL-3.0-only
//! Trust decision audit query layer: kylins' `trust_decisions.rs` ported.
//!
//! Owns the `trust_decisions` table, the **append-only** trust/acceptance
//! history for a peer key. Each [`put_trust_decision`] is an INSERT (never
//! UPDATE/DELETE); the latest decision for a key is read back by
//! [`get_latest_trust_decision`] ordering by `decided_at DESC, id DESC`.

use serde::{Deserialize, Serialize};

use crate::store::CryptoStore;

/// One trust-decision row. Mirrors the `trust_decisions` table; JSON keys
/// are camelCase for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrustDecisionRow {
    /// Autoincrement row id (the same-second tie-break).
    pub id: i64,
    /// The account the decision belongs to.
    pub account_id: String,
    /// The peer address the decision names (empty for unknown-key dialogs).
    pub peer_email: String,
    /// `'openpgp' | 'smime' | 'sm'`.
    pub standard: String,
    /// The key the decision is about.
    pub fingerprint: String,
    /// `'rejected' | 'undecided' | 'unverified' | 'verified' | 'personal'`.
    pub decision: String,
    /// Free-form provenance JSON, when the caller recorded any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_json: Option<String>,
    /// Unix-seconds string, stamped server-side at insert.
    pub decided_at: String,
}

const COLS: &str =
    "id, account_id, peer_email, standard, fingerprint, decision, evidence_json, decided_at";

fn row_to_trust_decision(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrustDecisionRow> {
    Ok(TrustDecisionRow {
        id: row.get(0)?,
        account_id: row.get(1)?,
        peer_email: row.get(2)?,
        standard: row.get(3)?,
        fingerprint: row.get(4)?,
        decision: row.get(5)?,
        evidence_json: row.get(6)?,
        decided_at: row.get(7)?,
    })
}

/// Append a new trust decision row (INSERT only; the table is an audit log,
/// so history is never mutated). `decided_at` is stamped server-side via
/// `strftime('%s','now')`.
///
/// # Errors
/// Fails when the insert cannot run.
pub fn put_trust_decision(
    store: &CryptoStore,
    account_id: &str,
    peer_email: &str,
    standard: &str,
    fingerprint: &str,
    decision: &str,
    evidence_json: Option<&str>,
) -> Result<(), String> {
    store
        .conn()
        .execute(
            "INSERT INTO trust_decisions
             (account_id, peer_email, standard, fingerprint, decision, evidence_json, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s','now'))",
            rusqlite::params![
                account_id,
                peer_email,
                standard,
                fingerprint,
                decision,
                evidence_json
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Return the latest trust decision for a peer key, or `None`. "Latest" is
/// `ORDER BY decided_at DESC, id DESC LIMIT 1` so a tie in `decided_at`
/// (second-resolution) breaks by insertion id.
///
/// # Errors
/// Fails on a database error.
pub fn get_latest_trust_decision(
    store: &CryptoStore,
    account_id: &str,
    peer_email: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<Option<TrustDecisionRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM trust_decisions
         WHERE account_id = ?1 AND peer_email = ?2 AND standard = ?3 AND fingerprint = ?4
         ORDER BY decided_at DESC, id DESC LIMIT 1"
    );
    store
        .conn()
        .query_row(
            &sql,
            rusqlite::params![account_id, peer_email, standard, fingerprint],
            row_to_trust_decision,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
}

/// Return the latest trust decision **per `peer_email`** for a key: one row
/// per distinct `peer_email` recorded under `(account_id, standard,
/// fingerprint)`. Backs the "Manage acceptance" dialog and the send-side
/// `accepted_elsewhere` detection.
///
/// Implementation: full `ORDER BY decided_at DESC, id DESC` scan of the
/// bucket with a Rust-side first-wins dedupe by `peer_email`; the first
/// occurrence of each email in that ordering IS the latest decision for it.
/// Ordered by `peer_email` on return for deterministic UI output.
///
/// # Errors
/// Fails on a database error.
pub fn list_latest_decisions_for_key(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    fingerprint: &str,
) -> Result<Vec<TrustDecisionRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM trust_decisions
         WHERE account_id = ?1 AND standard = ?2 AND fingerprint = ?3
         ORDER BY decided_at DESC, id DESC"
    );
    let conn = store.conn();
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params![account_id, standard, fingerprint],
            row_to_trust_decision,
        )
        .map_err(|e| e.to_string())?;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<TrustDecisionRow> = Vec::new();
    for row in rows {
        let row = row.map_err(|e| e.to_string())?;
        if seen.insert(row.peer_email.clone()) {
            out.push(row);
        }
    }
    out.sort_by(|a, b| a.peer_email.cmp(&b.peer_email));
    Ok(out)
}

/// Full decision history for one ADDRESS: every row recorded under
/// `(account_id, standard, peer_email)`, latest first (`decided_at DESC, id
/// DESC`). The key-continuity (anti-spoof) guard reads this to detect "this
/// address previously used a DIFFERENT key" before accepting a new
/// fingerprint.
///
/// Unlike [`list_latest_decisions_for_key`] this is NOT deduped: the
/// continuity check needs the superseded rows.
///
/// # Errors
/// Fails on a database error.
pub fn list_decisions_for_email(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    peer_email: &str,
) -> Result<Vec<TrustDecisionRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM trust_decisions
         WHERE account_id = ?1 AND standard = ?2 AND peer_email = ?3
         ORDER BY decided_at DESC, id DESC"
    );
    let conn = store.conn();
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params![account_id, standard, peer_email],
            row_to_trust_decision,
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod trust_tests;
