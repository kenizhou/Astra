// SPDX-License-Identifier: GPL-3.0-only
//! The outbox half of the inbound protocol ([`OutboxIntent`]), which
//! [`Intent::Outbox`] carries.
//!
//! Split from [`super::intent`] to keep each file under the 500-line limit,
//! following [`DraftsIntent`](super::DraftsIntent). The outbox reads as a unit
//! for the reason drafts do: one list, and four verbs that mean nothing
//! anywhere else in the app, because no other row names a message the server
//! has never seen.

use serde::{Deserialize, Serialize};

use crate::reference::QueuedRef;

/// What the outbox panel asks the runtime to do.
///
/// Every action but the show names a [`QueuedRef`]: an account bound to the
/// durable op in its queue. An op id is unique only within its account's
/// outbox, and the panel holds every account's sends at once, so a bare id
/// would be resolved against whichever account happened to be selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxIntent {
    /// Show the sends that have not gone out, across every account.
    Show,
    /// Withdraw a queued send so it is never delivered, or dismiss one the
    /// server refused.
    ///
    /// Refused by the store while the send is actually in flight, because
    /// stopping something already on its way is not a promise this app can
    /// keep, and while it awaits confirmation, because it may already have
    /// been delivered.
    Cancel {
        /// The queued send.
        queued: QueuedRef,
    },
    /// Attempt a queued send now, rather than waiting out its backoff, or send
    /// again one the server refused.
    ///
    /// Does **not** reset the attempt count: one more attempt now is not a
    /// fresh bound. Never sends a message awaiting confirmation; only
    /// [`ConfirmNotSent`](Self::ConfirmNotSent) does, so a row that changed
    /// state under the user's click cannot deliver it twice.
    SendNow {
        /// The queued send.
        queued: QueuedRef,
    },
    /// The user's answer to a send whose delivery could not be confirmed: it
    /// reached its recipients. It settles and leaves the outbox, and is never
    /// sent again.
    ConfirmSent {
        /// The queued send.
        queued: QueuedRef,
    },
    /// The user's answer to a send whose delivery could not be confirmed: it
    /// did not reach them. It goes back in the queue and is sent now.
    ///
    /// Refused by the store on a send that is not awaiting confirmation, so
    /// this is the one way a message that may have been delivered is ever
    /// attempted again.
    ConfirmNotSent {
        /// The queued send.
        queued: QueuedRef,
    },
    /// The user's answer to a prompt-user gate: Send Anyway queues the held
    /// composition without re-gating (the answer is never remembered), Deny
    /// drops the hold and keeps the draft unsent.
    AnswerGate {
        /// The held send the prompt names.
        hold: String,
        /// True for Send Anyway, false for Deny.
        send_anyway: bool,
    },
    /// Re-runs the gate over a held send: the retry for a handler that was
    /// unavailable or a recipient that has since become trusted.
    RetryHeld {
        /// The held send.
        hold: String,
    },
    /// Drops a held send. The draft behind it stays in Drafts; nothing is
    /// ever queued by withdrawing.
    WithdrawHeld {
        /// The held send.
        hold: String,
    },
}
