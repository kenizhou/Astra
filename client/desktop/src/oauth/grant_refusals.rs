// SPDX-License-Identifier: GPL-3.0-only
//! The standing grant refusals: one map of every Microsoft account whose
//! OAuth grant needs the person to act, the state the refusal surfaces
//! render. The state is **standing**, so it rides the settings snapshot's
//! account rows (the host's account-info seam reads it) rather than the
//! sign-in completion's one-shot event: an accounts page or folder pane
//! that mounts late, or a relaunch, must read it from a pull, and the
//! boot dial re-learns it every launch because the classification is
//! born where the dial runs.
//!
//! Who writes it:
//!
//! - **The boot dial**: a dead grant (`SigninRejected`) records the
//!   expired sign-in for the account the boot then skips, so the row that
//!   renders for it is the only surface explaining the absence; a dialled
//!   calendar probe that answered `calendar_reauth_required` records the
//!   withheld calendar; a healthy dial records nothing.
//! - **The sign-in's completion**: a grant that just landed records what
//!   it bears (a withheld calendar records, a healthy grant clears), and
//!   a refused completion records nothing because its rollback rule
//!   leaves everything that stood exactly as it was.
//! - **The removal**: a removed account owes no refusal state.
//!
//! The words are [`astra_viewmodel::GrantState`], the wire's own type: the
//! map stores what the rows cross, with no second vocabulary to drift.

use std::collections::BTreeMap;
use std::sync::Mutex;

use astra_viewmodel::GrantState;

use super::commands::RefusalKind;

/// Why the completion refused: the ported classification word beside the
/// host's own sentence, the shape the sign-in outcome's refusal arm
/// carries to the surface. The one-shot twin of the standing map: the
/// completion answers the wizard that began it, the map is what every
/// later surface reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GraphAddFailure {
    /// The ported classification of the refusal.
    pub(crate) refusal: RefusalKind,
    /// The host's own sentence.
    pub(crate) detail: String,
}

/// A refusal nothing was learned from: the rollback guarantees nothing
/// changed about any grant that stands.
pub(crate) fn indeterminate(detail: String) -> GraphAddFailure {
    GraphAddFailure {
        refusal: RefusalKind::Indeterminate,
        detail,
    }
}

/// Every Microsoft account's standing grant refusal, keyed by the account
/// id the dial table holds. Shared behind one `Arc` between the boot dial
/// that writes it, the completion that rewrites it, and the host account
/// store the settings rows read it through.
pub(crate) struct GrantRefusals {
    /// The standing states, keyed by account id.
    entries: Mutex<BTreeMap<String, GrantState>>,
}

impl GrantRefusals {
    /// An empty map: the state every boot starts from.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Records one account's standing state.
    pub(crate) fn record(&self, account: &str, state: GrantState) {
        self.entries
            .lock()
            .expect("grant-refusals mutex poisoned")
            .insert(account.to_owned(), state);
    }

    /// Records the expired sign-in for an account whose grant the dial
    /// classified dead.
    pub(crate) fn record_dead(&self, account: &str) {
        self.record(account, GrantState::SignInExpired);
    }

    /// Records what a dialled grant bears: a withheld calendar records
    /// the reconnect state, a healthy grant clears whatever stood, because
    /// a dial that answered proves the standing state wrong.
    pub(crate) fn record_dialled(&self, account: &str, calendar_withheld: bool) {
        if calendar_withheld {
            self.record(account, GrantState::CalendarWithheld);
        } else {
            self.clear(account);
        }
    }

    /// Clears one account's standing state.
    pub(crate) fn clear(&self, account: &str) {
        self.entries
            .lock()
            .expect("grant-refusals mutex poisoned")
            .remove(account);
    }

    /// The account's standing state, or `None` when its grant is healthy.
    pub(crate) fn of(&self, account: &str) -> Option<GrantState> {
        self.entries
            .lock()
            .expect("grant-refusals mutex poisoned")
            .get(account)
            .copied()
    }
}

impl Default for GrantRefusals {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "grant_refusals_tests.rs"]
mod tests;
