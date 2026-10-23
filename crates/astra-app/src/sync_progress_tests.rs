// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sync-progress fold: what each engine event moves, that a snapshot
//! publishes only when it changed, and the fold a whole pass leaves behind.

use std::sync::{Arc, Mutex};

use astra_viewmodel::AccountSyncState;
use engine_api::{AccountId, MailboxId, SyncObserver, SyncScope};

use super::SyncProgress;
use crate::{AppObserver, Surface};

/// Records the surfaces the fold signals.
struct RecordingObserver {
    surfaces: Arc<Mutex<Vec<Surface>>>,
}

impl AppObserver for RecordingObserver {
    fn surface_changed(&self, surface: Surface) {
        self.surfaces.lock().unwrap().push(surface);
    }
}

/// A fold over a recording observer.
fn fold() -> (SyncProgress, Arc<Mutex<Vec<Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let observer = RecordingObserver {
        surfaces: Arc::clone(&surfaces),
    };
    (
        SyncProgress::new(
            Arc::new(observer),
            Arc::new(crate::notifications::NewMailQueue::new()),
        ),
        surfaces,
    )
}

fn account() -> AccountId {
    AccountId::try_from("user@example.test").expect("an account id")
}

/// Any scope: the fold reads no scope field, so one easy-to-build scope stands in.
fn scope() -> SyncScope {
    SyncScope::ImapMailboxList { account: account() }
}

#[test]
fn a_pass_folds_start_position_and_end() {
    let (progress, surfaces) = fold();
    let account = account();

    SyncObserver::account_sync_started(
        &progress,
        &account,
        10,
        Some(&MailboxId::try_from("INBOX").expect("a mailbox id")),
    );
    let syncing = progress.snapshot();
    assert_eq!(syncing.accounts.len(), 1, "the account reports");
    assert_eq!(syncing.accounts[0].state, AccountSyncState::Syncing);
    assert_eq!(
        syncing.accounts[0].detail.as_deref(),
        Some("folders 0 of 10"),
    );

    // Eight folders finish, one of them having failed: the position advances either way.
    for at in 0..8 {
        SyncObserver::folder_sync_finished(&progress, &account, &scope(), at != 7);
    }
    let advanced = progress.snapshot();
    assert_eq!(
        advanced.accounts[0].detail.as_deref(),
        Some("folders 8 of 10")
    );

    SyncObserver::account_sync_finished(&progress, &account);
    let finished = progress.snapshot();
    assert_eq!(finished.accounts[0].state, AccountSyncState::Idle);

    // Every event moved the fold, so every one published: a signal per change.
    assert_eq!(
        surfaces.lock().unwrap().len(),
        10,
        "start + eight folders + end"
    );
    assert!(surfaces
        .lock()
        .unwrap()
        .iter()
        .all(|surface| matches!(surface, Surface::SyncProgress)));
}

#[test]
fn a_second_pass_of_the_same_account_restarts_its_position() {
    let (progress, _surfaces) = fold();
    let account = account();
    SyncObserver::account_sync_started(&progress, &account, 10, None);
    SyncObserver::folder_sync_finished(&progress, &account, &scope(), true);
    SyncObserver::account_sync_finished(&progress, &account);

    SyncObserver::account_sync_started(&progress, &account, 4, None);
    let restarted = progress.snapshot();

    assert_eq!(restarted.accounts.len(), 1, "one account, not two entries");
    assert_eq!(restarted.accounts[0].state, AccountSyncState::Syncing);
    assert_eq!(
        restarted.accounts[0].detail.as_deref(),
        Some("folders 0 of 4")
    );
}

#[test]
fn an_event_that_moves_nothing_publishes_nothing() {
    let (progress, surfaces) = fold();
    let account = account();

    // An account nobody has started: finishing a folder of it still creates the entry
    // (the engine always pairs a start first, but the fold stays honest alone), so
    // this moves the fold and publishes once.
    SyncObserver::folder_sync_finished(&progress, &account, &scope(), false);
    assert_eq!(surfaces.lock().unwrap().len(), 1);

    // The same event again would move the count; the one that cannot move anything is
    // a finish for a folder of an account whose pass already ended at that count.
    SyncObserver::account_sync_finished(&progress, &account);
    let before = surfaces.lock().unwrap().len();
    SyncObserver::account_sync_finished(&progress, &account);
    assert_eq!(
        surfaces.lock().unwrap().len(),
        before,
        "a second end event publishes nothing"
    );
}

#[test]
fn accounts_stay_in_account_order_whatever_the_events_arrive_in() {
    let (progress, _surfaces) = fold();
    let late = AccountId::try_from("aaa@early.example.test").expect("an account id");
    let early = AccountId::try_from("zzz@late.example.test").expect("an account id");

    SyncObserver::account_sync_started(&progress, &early, 2, None);
    SyncObserver::account_sync_started(&progress, &late, 3, None);

    let snapshot = progress.snapshot();
    assert_eq!(
        snapshot
            .accounts
            .iter()
            .map(|entry| entry.account.as_str())
            .collect::<Vec<_>>(),
        vec!["aaa@early.example.test", "zzz@late.example.test"],
        "the snapshot is ordered by account id, not by arrival",
    );
}

#[test]
fn a_fresh_fold_snapshots_empty_and_signals_nothing() {
    let (progress, surfaces) = fold();

    assert!(progress.snapshot().accounts.is_empty());
    assert!(surfaces.lock().unwrap().is_empty());
}
