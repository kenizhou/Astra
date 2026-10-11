// SPDX-License-Identifier: GPL-3.0-only
//! The search view-model: the scope a search covers and the state it publishes.
//!
//! Pure projection, like the mailbox list beside it: the app runs the search
//! and folds what it found into this state, and every surface shares one
//! definition of what a search shows. The rows are the same shape the mailbox
//! list uses, because a search is a mode over the list, not a second kind of
//! list: whatever grouping the list is in, the results follow it.

use serde::{Deserialize, Serialize};

use crate::rows::MessageRowView;

/// What a search covers: how far beyond the folder on screen it reaches.
///
/// Defined in the view-model, beside the search state that carries it, so the
/// intent that changes the scope and the snapshot that echoes it name one
/// type; re-exported by the application core's protocol to keep that the one
/// import a host needs, the same arrangement [`ViewMode`](crate::ViewMode)
/// has.
///
/// The default reaches everywhere but Trash: a person looking for a message
/// knows roughly when it arrived and rarely which folder it ended up in, while
/// a deleted copy sitting beside the live one is noise. The exclusion is a
/// default, not a wall: Trash stays reachable by narrowing to the current
/// folder or account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    /// Every account, every folder, except each account's Trash. The default.
    #[default]
    AllExceptTrash,
    /// Only what the mailbox list was showing when the search started: the
    /// selected folder, or (in the unified view) every account's Inbox.
    /// Mirrors the list exactly, so searching inside Trash finds trashed mail.
    CurrentFolder,
    /// The selected account's whole mailbox, Trash included; in the unified
    /// view, every account's mailbox.
    CurrentAccount,
}

/// The state of the mailbox list while a search runs: what was asked for,
/// what it covered, and what it found.
///
/// Carried by [`MailboxListSnapshot`](crate::view::MailboxListSnapshot) as an
/// additive field, present exactly while a search is active, so the same
/// snapshot serves the folder view and the search view and an emitter that
/// searches nothing crosses unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchState {
    /// The query exactly as the user typed it, prefixes and all: the echo the
    /// field shows, not the split the search ran with.
    pub query: String,
    /// The scope the search ran under, echoed so a surface draws the filter
    /// the results actually honour.
    pub scope: SearchScope,
    /// The results, newest first, grouped exactly as the mailbox list groups:
    /// a flat row per message, or a thread's summary carrying the whole
    /// conversation.
    pub rows: Vec<MessageRowView>,
    /// How far back the search looked, in whole months: the narrowest sync
    /// depth among the accounts the scope covered. Results never claim beyond
    /// it, so a surface states the horizon beside them rather than letting an
    /// empty answer claim there is no such message.
    pub depth_months: u32,
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
