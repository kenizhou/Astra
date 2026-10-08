// SPDX-License-Identifier: GPL-3.0-only
//! The sync-progress view-model: an immutable snapshot of which accounts have a mail
//! sync pass in flight and how far each one has reached.
//!
//! Pure projection, like the mailbox list beside it: state lives in the engine, the
//! app folds each pass's per-account position into this snapshot, and every surface
//! shares one definition of what "syncing" looks like. A host learns the snapshot
//! changed from a `sync_progress` signal and pulls this; the snapshot carries no
//! payload across the signal itself, so a coalesced burst of signals always reports
//! where the sync actually is when the host reads it.

use engine_api::AccountId;
use serde::{Deserialize, Serialize};

/// One account's position within the sync-progress snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSyncProgress {
    /// The account this entry reports on.
    pub account: AccountId,
    /// Whether a pass is running for the account, or its last pass has ended.
    pub state: AccountSyncState,
    /// How far the account's pass is (for example `folders 3 of 10`), when a position
    /// is known. A display hint, not localised text: the surface decides what to draw
    /// and in which language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// An account's position between sync passes. Crosses the IPC as a snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountSyncState {
    /// A pass is syncing the account's folders.
    Syncing,
    /// The account's last pass has ended. The entry stays published so a bar that was
    /// showing the account clears in step with the others; nothing further moves it
    /// until the account's next pass starts.
    Idle,
}

/// The sync-progress snapshot a host pulls after a `sync_progress` signal: one entry
/// per account the app has run a pass for this run, in account order.
///
/// Empty until the first pass starts: an app that has not synced anything has no
/// progress to report, and a bar drawn from an empty snapshot stays down.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncProgressSnapshot {
    /// One entry per account with a sync position to report.
    pub accounts: Vec<AccountSyncProgress>,
}

#[cfg(test)]
#[path = "sync_progress_tests.rs"]
mod tests;
