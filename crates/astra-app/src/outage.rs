// SPDX-License-Identifier: GPL-3.0-only
//! The outage state: the per-account fold over pass outcomes that the
//! offline banner and the pane's outage badges read. The host's background
//! loop makes the typed record call, and the ordinary pass paths make it
//! too, so a manual refresh clears a standing outage exactly as a
//! background pass does.
//!
//! The rule, decided once: a streak of two or more consecutive
//! network-class failures puts the account in outage; any success clears
//! it; a permanent failure never counts as one and clears a standing
//! streak, because an account whose sign-in died is a dead account the
//! accounts page reports, not an offline one the banner claims. Store
//! faults never count either, for the reason the pass report itself
//! records: a broken store that reported itself as an outage would send
//! the person to check their wifi.

use std::{collections::BTreeMap, sync::Mutex};

use astra_viewmodel::DeliveryState;
use engine_api::{AccountId, FailureClass, MailSyncReport, SyncError};

use crate::App;

/// How many consecutive network-class failures put an account in outage.
pub(crate) const OUTAGE_AFTER: u32 = 2;

/// One account's mail-pass outcome, as the delivery state records it: the
/// classification the loop owns, folded from the pass report the engine
/// answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassOutcome {
    /// The pass reached the server and applied what it brought: any
    /// standing outage streak clears.
    Success,
    /// A retryable network-class failure: the streak the outage counts.
    NetworkFailure,
    /// A permanent provider refusal: the account is dead, never offline.
    PermanentFailure,
}

impl PassOutcome {
    /// The classification of one pass's report: a network-class failure
    /// wins over a permanent one (a pass that hit both may well be offline;
    /// the account that cannot be reached at all is the louder fact), busy
    /// scopes and store faults say nothing and leave the pass a success for
    /// outage purposes, because neither is evidence about reachability.
    #[must_use]
    pub fn of(report: &MailSyncReport) -> Self {
        let mut network = false;
        let mut refused = false;
        let errors = report
            .account_steps
            .as_ref()
            .err()
            .into_iter()
            .chain(
                report
                    .mailboxes
                    .as_ref()
                    .and_then(|step| step.as_ref().err()),
            )
            .chain(
                report
                    .folders
                    .iter()
                    .filter_map(|folder| folder.result.as_ref().err()),
            );
        for error in errors {
            match error {
                SyncError::Provider(err) if err.class() == FailureClass::Retryable => {
                    network = true;
                }
                SyncError::Provider(_) => refused = true,
                _ => {}
            }
        }
        if network {
            Self::NetworkFailure
        } else if refused {
            Self::PermanentFailure
        } else {
            Self::Success
        }
    }
}

/// The per-account outage streaks, behind one lock the record call folds.
#[derive(Default)]
pub(crate) struct Outages {
    /// Each dialled account's consecutive network-class failure count; an
    /// absent entry is an account never dialled in this process, or one
    /// whose last word was death (it left the dialled set).
    streaks: Mutex<BTreeMap<AccountId, u32>>,
}

/// The outage state shared between the app and the progress fold that
/// projects it: one state, one projection.
pub(crate) type OutagesShared = std::sync::Arc<Outages>;

impl Outages {
    /// A state with nothing dialled.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Folds one pass outcome into the account's streak: a success clears
    /// but keeps the account dialled (a healthy account is what keeps the
    /// banner off), a network failure counts, and a permanent failure
    /// leaves the dialled set entirely, because a dead account is no longer
    /// dialled and must never wear the offline badge.
    pub(crate) fn record(&self, account: &AccountId, outcome: PassOutcome) {
        let mut streaks = self.streaks.lock().expect("outage mutex poisoned");
        match outcome {
            PassOutcome::Success => {
                streaks.insert(account.clone(), 0);
            }
            PassOutcome::PermanentFailure => {
                streaks.remove(account);
            }
            PassOutcome::NetworkFailure => {
                let streak = streaks.entry(account.clone()).or_insert(0);
                *streak = streak.saturating_add(1);
            }
        }
    }

    /// Whether the account is in outage: a streak of
    /// [`OUTAGE_AFTER`] or more network-class failures.
    pub(crate) fn in_outage(&self, account: &AccountId) -> bool {
        let streaks = self.streaks.lock().expect("outage mutex poisoned");
        streaks
            .get(account)
            .is_some_and(|streak| *streak >= OUTAGE_AFTER)
    }

    /// Whether the offline banner shows: every dialled account in outage
    /// and at least one dialled.
    pub(crate) fn banner(&self) -> bool {
        let streaks = self.streaks.lock().expect("outage mutex poisoned");
        !streaks.is_empty() && streaks.values().all(|streak| *streak >= OUTAGE_AFTER)
    }

    /// The delivery state the sync-progress snapshot carries: the banner
    /// answer beside the accounts in outage, in account order, so the
    /// progress pull and the outage badge rows read one projection.
    pub(crate) fn delivery(&self) -> DeliveryState {
        let streaks = self.streaks.lock().expect("outage mutex poisoned");
        DeliveryState {
            offline_banner: !streaks.is_empty()
                && streaks.values().all(|streak| *streak >= OUTAGE_AFTER),
            outages: streaks
                .iter()
                .filter(|(_, streak)| **streak >= OUTAGE_AFTER)
                .map(|(account, _)| account.clone())
                .collect(),
        }
    }
}

impl<P> App<P> {
    /// Records one account's pass outcome into the outage state: the typed
    /// call the background loop makes for its own outcomes (a watch error,
    /// a reconnect) beside the ones the pass paths record for it. A move
    /// in the state the surfaces read republishes the sync-progress
    /// snapshot, so the offline banner and the outage badges land on the
    /// channel the progress bar already crosses.
    pub fn record_pass_outcome(&self, account: &AccountId, outcome: PassOutcome) {
        self.outages.record(account, outcome);
        self.sync_progress.republish();
    }

    /// Whether one account is in outage: the badge a pane row carries.
    #[must_use]
    pub fn account_in_outage(&self, account: &AccountId) -> bool {
        self.outages.in_outage(account)
    }

    /// Whether the offline banner shows: every dialled account in outage
    /// and at least one dialled.
    #[must_use]
    pub fn offline_banner(&self) -> bool {
        self.outages.banner()
    }
}

#[cfg(test)]
#[path = "outage_tests.rs"]
mod tests;
