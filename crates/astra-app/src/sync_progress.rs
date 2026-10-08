// SPDX-License-Identifier: GPL-3.0-only
//! Sync progress, app side: the engine [`SyncObserver`] seam every mail pass reports
//! through, folding each pass's per-account position into one
//! [`SyncProgressSnapshot`] cell.
//!
//! The engine reports a pass's lifecycle (`account_sync_started`, one
//! `folder_sync_finished` per folder whatever the outcome, `account_sync_finished`);
//! this module is the whole app policy around it: fold those events per account,
//! publish the snapshot **only when the fold moved it**, and nothing else. The
//! folder-count fold is the SP1 contract; message-count aggregation joins when the
//! message list surface exists.
//!
//! The cell is a [`Surfaced`], so the publish-then-signal ordering the host's pull
//! depends on is the same one the mailbox list already rests on.

use std::{collections::BTreeMap, sync::Mutex};

use astra_viewmodel::{AccountSyncProgress, AccountSyncState, SyncProgressSnapshot};
use engine_api::{AccountId, MailboxId, SyncCommit, SyncObserver, SyncScope};

use crate::{AppObserver, Surface, Surfaced};

/// One account's folded position, behind the snapshot's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FoldedAccount {
    /// Whether the account's pass is running or has ended.
    state: AccountSyncState,
    /// How many of the pass's folders have finished, whatever their outcome: a count
    /// that only advanced on success would stall at "3 of 10" over a failed folder.
    folders_done: u32,
    /// How many folders the pass set out to sync, when the engine has said.
    folders_total: Option<u32>,
}

/// The sync-progress fold: one shared cell every account's pass reports into,
/// publishing a [`SyncProgressSnapshot`] on change.
///
/// Shared by every concurrent pass (a [`RefreshMail`](crate::Intent::RefreshMail)
/// syncs all accounts at once), so the snapshot is one consistent whole rather than
/// per-pass fragments that would each clear the others.
pub(crate) struct SyncProgress {
    /// The published snapshot and its signal.
    cell: Surfaced<SyncProgressSnapshot>,
    /// Every account's folded position, keyed by id so the snapshot's order is stable.
    accounts: Mutex<BTreeMap<AccountId, FoldedAccount>>,
}

impl SyncProgress {
    /// An unpublished fold over no accounts, signalling `surface` on `observer`.
    pub(crate) fn new(observer: std::sync::Arc<dyn AppObserver>) -> Self {
        Self {
            cell: Surfaced::new(Surface::SyncProgress, observer),
            accounts: Mutex::new(BTreeMap::new()),
        }
    }

    /// The current snapshot, for the host's pull.
    pub(crate) fn snapshot(&self) -> SyncProgressSnapshot {
        Self::project(&self.accounts.lock().expect("sync-progress mutex poisoned"))
    }

    /// Applies `change` to `account`'s fold and publishes when the snapshot moved.
    ///
    /// Publishing only on change keeps a pass's finishing events from re-announcing a
    /// snapshot identical to the last one; the host's debounce coalesces the rest.
    fn fold(&self, account: &AccountId, change: impl FnOnce(&mut FoldedAccount)) {
        let mut accounts = self.accounts.lock().expect("sync-progress mutex poisoned");
        let before = Self::project(&accounts);
        let entry = accounts
            .entry(account.clone())
            .or_insert_with(|| FoldedAccount {
                state: AccountSyncState::Syncing,
                folders_done: 0,
                folders_total: None,
            });
        change(entry);
        let after = Self::project(&accounts);
        drop(accounts);
        if before != after {
            self.cell.publish(after);
        }
    }

    /// Projects the fold into the snapshot a host pulls, in account order.
    fn project(accounts: &BTreeMap<AccountId, FoldedAccount>) -> SyncProgressSnapshot {
        SyncProgressSnapshot {
            accounts: accounts
                .iter()
                .map(|(account, folded)| AccountSyncProgress {
                    account: account.clone(),
                    state: folded.state,
                    detail: folded
                        .folders_total
                        .map(|total| format!("folders {} of {total}", folded.folders_done)),
                })
                .collect(),
        }
    }
}

impl SyncObserver for SyncProgress {
    /// Deliberately empty: the SP1 fold is folder-level, and a commit carries no
    /// folder position this snapshot reports. The engine requires the method; leaving
    /// it a no-op is the honest statement that commits do not move this surface yet.
    fn committed(&self, _commit: &SyncCommit<'_>) {}

    /// Starts the account's pass: its fold resets to zero of `folders`, so a second
    /// pass of the same account restarts its position rather than resuming the last.
    fn account_sync_started(
        &self,
        account: &AccountId,
        folders: usize,
        _inbox: Option<&MailboxId>,
    ) {
        let total = u32::try_from(folders).ok();
        self.fold(account, |entry| {
            *entry = FoldedAccount {
                state: AccountSyncState::Syncing,
                folders_done: 0,
                folders_total: total,
            };
        });
    }

    /// Counts one of the account's folders as finished, synced or not: the position is
    /// what the bar divides, and a folder that failed is no longer pending either way.
    fn folder_sync_finished(&self, account: &AccountId, _scope: &SyncScope, _synced: bool) {
        self.fold(account, |entry| {
            entry.folders_done = entry.folders_done.saturating_add(1);
        });
    }

    /// Ends the account's pass: the entry stays published, now `idle`, so a bar that
    /// was showing the account clears in step with the accounts still syncing.
    fn account_sync_finished(&self, account: &AccountId) {
        self.fold(account, |entry| {
            entry.state = AccountSyncState::Idle;
        });
    }
}

#[cfg(test)]
#[path = "sync_progress_tests.rs"]
mod tests;
