// SPDX-License-Identifier: GPL-3.0-only
//! The sending view-model: the immutable snapshot of the sends that have not
//! gone out, for a surface to render as the outbox panel.
//!
//! A queued send is not a stored message, and the two must not share a row
//! type: it has no provider key (nothing on a server has it yet), no read or
//! flagged state, and no received date. Its identity is the durable outbox op,
//! and the interesting facts about it are the ones a stored message never
//! has: where the send stands, and when it joined the queue.

use engine_api::{queued_draft, AccountId, PendingOpRow, PendingOpState};
use serde::{Deserialize, Serialize};

/// The identity of one queued send: its owning account and its outbox op id
/// together.
///
/// An op id is unique only within its account's outbox, so the account is part
/// of the identity: the pair is what a surface hands back to name the send an
/// action acts on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueuedKey {
    /// The account whose outbox holds the send.
    pub account: AccountId,
    /// The durable op's id, as the store assigned it.
    pub op: u64,
}

/// Where a queued send has got to.
///
/// Coarser than the store's `PendingOpState` on purpose: a surface shows a
/// person what is happening to their message, not the lifecycle of an op. A
/// send that has not been tried and one waiting out a backoff read the same,
/// because the difference is a delay the user did not choose and cannot act
/// on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedState {
    /// Waiting to go: queued, or backing off between attempts.
    Queued,
    /// A send is in flight right now.
    Sending,
    /// The send may or may not have been delivered, and will never be retried
    /// automatically. Only a reconcile or the user's answer resolves it.
    ///
    /// Its own state because it is the one queued row where "send now" is the
    /// wrong offer: the message may already be in front of its recipients.
    Unconfirmed,
    /// The server refused it, and nothing will try again on its own.
    ///
    /// Kept rather than dropped: it is the user's message, and the queue may
    /// be the only place left holding it. It stays until they send it again or
    /// withdraw it.
    Failed,
}

impl QueuedState {
    /// The row state an outbox op reads as, or `None` for one that is no
    /// longer outstanding (settled or withdrawn): those leave the queue and
    /// have no row to draw.
    #[must_use]
    pub fn of(state: PendingOpState) -> Option<Self> {
        match state {
            PendingOpState::Pending => Some(Self::Queued),
            PendingOpState::InFlight => Some(Self::Sending),
            PendingOpState::NeedsConfirmation => Some(Self::Unconfirmed),
            PendingOpState::Failed => Some(Self::Failed),
            PendingOpState::Succeeded | PendingOpState::Cancelled => None,
        }
    }
}

/// One queued send, as the outbox panel shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedRow {
    /// The send's identity: what an action names to act on this row.
    pub queued: QueuedKey,
    /// Where the send has got to.
    pub state: QueuedState,
    /// The subject line, empty if the draft carried none.
    pub subject: String,
    /// When the send joined the queue, as a whole-second UTC `…Z` instant
    /// (`2026-10-09T08:30:00Z`); empty when the app did not see it arrive. A
    /// surface localises it for display.
    pub arrived_at: String,
}

/// Where a held send stands: the gate states a surface renders differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeldState {
    /// The gate is running; the plugin named is being awaited.
    Checking,
    /// A prompt-user handler asked a question; the row waits on the person's
    /// Send Anyway or Deny.
    NeedsAnswer,
    /// A block-mode handler refused, timed out, or was unavailable: the send
    /// holds until the person retries or withdraws it.
    Blocked,
}

/// One send the plugin gate is holding back from the queue.
///
/// A held send is not a queued send: the engine never saw it, which is what
/// makes "no plaintext fallback" a construction fact rather than a
/// convention. The composition behind it stays a saved draft; the row's id is
/// what an answer (Send Anyway, Deny) or a retry names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldSend {
    /// The hold's id, minted when the gate took the composition.
    pub hold: String,
    /// The account the send would leave from.
    pub account: AccountId,
    /// The subject line, empty if the draft carried none.
    pub subject: String,
    /// The plugin whose verdict is awaited or whose refusal holds the send.
    pub plugin: String,
    /// The reason the handler gave (or the timeout's), in product words.
    pub reason: String,
    /// Where the hold stands.
    pub state: HeldState,
}

/// An immutable snapshot of the sends that have not gone out, for a surface to
/// render as the outbox panel.
///
/// Every account's sends in one list, in account order then queue order: the
/// queue is the one place a person looks for mail that has not gone, whichever
/// account it was to leave from. Empty when every send has gone. The held leg
/// carries the sends the plugin gate is holding back from the queue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendingSnapshot {
    /// The outstanding sends, oldest queue position first within each account.
    pub rows: Vec<QueuedRow>,
    /// The sends the plugin gate is holding, in the order they were held.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<HeldSend>,
}

/// Projects one outbox row into the queue's row, or `None` for a row that is
/// not a send this list shows.
///
/// Only **sends** appear. A queued draft save or keyword change is a write on
/// a message that is already in a folder, and the folder is where the user
/// will look for it; a row for it in a list of unsent mail would be a second,
/// stranger copy of the same action. `arrived_at` is the caller's to stamp:
/// the store holds no arrival instant, so the app notes when each op first
/// appeared in its read.
#[must_use]
pub fn queued_row(
    account: &AccountId,
    row: &PendingOpRow,
    arrived_at: String,
) -> Option<QueuedRow> {
    // `queued_draft` refuses anything that is not a submission, which is also
    // the filter that keeps edits and saves out of the list.
    let draft = queued_draft(row)?;
    Some(QueuedRow {
        queued: QueuedKey {
            account: account.clone(),
            op: row.id.get(),
        },
        state: QueuedState::of(row.state)?,
        subject: draft.subject,
        arrived_at,
    })
}

#[cfg(test)]
#[path = "sending_tests.rs"]
mod tests;
