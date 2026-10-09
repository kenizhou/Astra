// SPDX-License-Identifier: GPL-3.0-only
//! Message-row types for the mailbox-list view-model: the key a row is matched
//! by, the grouping mode the rows are built in, and the row itself.
//!
//! One row shape serves both modes: a flat row is a single message and a
//! threaded row a conversation's summary, and the only difference a surface
//! needs beyond the shared fields is [`MessageRowView::thread_count`]. The
//! field names trim the ported row shapes to what a list row renders; the
//! projection that fills them lives in the application core, as the folder-row
//! projection does.
//!
//! [`ViewMode`] is defined here, beside the rows it groups, and re-exported by
//! the application core's protocol so an intent and the snapshot it changes
//! name one type for the same concept.

use engine_api::{AccountId, ProviderKey, ThreadId};
use serde::{Deserialize, Serialize};

/// How the mailbox list is grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// A flat list of messages, newest first.
    #[default]
    Flat,
    /// Conversations grouped by thread, newest activity first.
    Threaded,
}

/// The identity a mailbox-list row is matched by: the message or thread it
/// shows, bound to the account holding it.
///
/// A provider key or thread id is unique only within its account, so the
/// account is part of the key: a row of the unified inbox is named by the pair,
/// and two accounts' rows never collide. The selection a snapshot carries and
/// the row it selects are matched by exactly this value, so a surface answers
/// "is this row selected?" with one comparison and no string building.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKey {
    /// A single message, matched by its provider key.
    Message {
        /// The account the message belongs to.
        account: AccountId,
        /// The message's provider key: its stable identity across mailbox
        /// moves, unique within the account.
        key: ProviderKey,
    },
    /// A conversation, matched by its thread id.
    Thread {
        /// The account the conversation belongs to.
        account: AccountId,
        /// The thread's id: provider-assigned where the provider threads, and
        /// locally derived otherwise.
        thread_id: ThreadId,
    },
}

/// One mailbox-list row: a single message in flat mode, or a conversation's
/// summary in threaded mode.
///
/// The row carries display scalars only, so a surface renders it without
/// opening the message: who it is from, what it is about, when, and the three
/// states a list row draws. The sender appears twice, by design: the display
/// name is what a row shows, while the bare address is the identity an avatar
/// is of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRowView {
    /// The row's identity: what a selection or an action on the row names.
    ///
    /// A message's provider key in flat mode, a thread's id in threaded mode;
    /// always bound to the account holding it.
    pub key: RowKey,
    /// The account the row's mail belongs to.
    ///
    /// Carried beside [`key`](Self::key) so a surface reads it without
    /// matching on the key: the account is what a row is coloured or grouped
    /// by in a unified list, and it reaches for it on every row.
    pub account_id: AccountId,
    /// The sender's display name, falling back to their email address when the
    /// header carried no name, and empty when there is no sender at all.
    ///
    /// The friendlier half of the sender; the reading view carries the full
    /// `Name <email>`, a list row just the name.
    pub sender_name: String,
    /// The sender's bare email address, empty when there is no sender.
    ///
    /// Not for display; [`sender_name`](Self::sender_name) is what a row
    /// shows. This is the identity an avatar is of, kept so the surface can
    /// resolve one without re-deriving which sender the row names.
    pub sender_address: String,
    /// The subject, empty if none.
    pub subject: String,
    /// When the message was sent, as a whole-second UTC `…Z` instant
    /// (`2026-10-09T08:30:00Z`), empty when unknown.
    ///
    /// The same instant the row is ordered by: the delivery date, falling back
    /// to the `Date` header. A surface localises it for display; the truncated
    /// shape keeps it parseable by every platform's date parser.
    pub sent_at: String,
    /// Whether the message is unread.
    pub unread: bool,
    /// Whether the message is flagged.
    pub flagged: bool,
    /// Whether the message has a non-inline attachment.
    pub has_attachments: bool,
    /// How many messages the conversation holds, in threaded mode.
    ///
    /// Absent on a flat row, where there is no conversation to count; a
    /// surface draws the count stack only when the field is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_count: Option<u32>,
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
