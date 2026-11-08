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

use crate::{outage::OutagesShared, AppObserver, Surface, Surfaced};

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
    /// The new-mail queue every commit also feeds, shared with the app: one
    /// observer drives both folds, so a pass's commits cannot reach one and
    /// miss the other.
    new_mail: std::sync::Arc<crate::notifications::NewMailQueue>,
    /// The outage state the composed snapshot carries, shared with the app:
    /// the delivery fold reads it at projection time, so a pass outcome
    /// recorded outside a pass moves the snapshot too.
    outages: OutagesShared,
}

impl SyncProgress {
    /// An unpublished fold over no accounts, signalling `surface` on
    /// `observer`, feeding `new_mail` with every commit, carrying `outages`
    /// as the delivery state every projection reads.
    pub(crate) fn new(
        observer: std::sync::Arc<dyn AppObserver>,
        new_mail: std::sync::Arc<crate::notifications::NewMailQueue>,
        outages: OutagesShared,
    ) -> Self {
        Self {
            cell: Surfaced::new(Surface::SyncProgress, observer),
            accounts: Mutex::new(BTreeMap::new()),
            new_mail,
            outages,
        }
    }

    /// The current snapshot, for the host's pull: the account positions and
    /// the delivery state, one composed whole.
    pub(crate) fn snapshot(&self) -> SyncProgressSnapshot {
        self.composed(&self.accounts.lock().expect("sync-progress mutex poisoned"))
    }

    /// Republishes the composed snapshot when the delivery state moved it:
    /// the call a pass outcome recorded outside a pass makes, so the
    /// offline banner lands without a pass having to run.
    pub(crate) fn republish(&self) {
        let now = self.snapshot();
        if now != self.cell.get() {
            self.cell.publish(now);
        }
    }

    /// Applies `change` to `account`'s fold and publishes when the snapshot moved.
    ///
    /// Publishing only on change keeps a pass's finishing events from re-announcing a
    /// snapshot identical to the last one; the host's debounce coalesces the rest.
    fn fold(&self, account: &AccountId, change: impl FnOnce(&mut FoldedAccount)) {
        let mut accounts = self.accounts.lock().expect("sync-progress mutex poisoned");
        let before = self.composed(&accounts);
        let entry = accounts
            .entry(account.clone())
            .or_insert_with(|| FoldedAccount {
                state: AccountSyncState::Syncing,
                folders_done: 0,
                folders_total: None,
            });
        change(entry);
        let after = self.composed(&accounts);
        drop(accounts);
        if before != after {
            self.cell.publish(after);
        }
    }

    /// Projects the fold beside the standing delivery state into the
    /// snapshot a host pulls, in account order.
    fn composed(&self, accounts: &BTreeMap<AccountId, FoldedAccount>) -> SyncProgressSnapshot {
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
            delivery: self.outages.delivery(),
        }
    }
}

impl SyncObserver for SyncProgress {
    /// Forwards to the new-mail queue: this fold itself stays folder-level
    /// (a commit carries no folder position this snapshot reports), and the
    /// queue owns the whole message-level policy around a commit.
    fn committed(&self, commit: &SyncCommit<'_>) {
        self.new_mail.committed(commit);
    }

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
