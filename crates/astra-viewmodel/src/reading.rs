// SPDX-License-Identifier: GPL-3.0-only
//! The reading view-model: the identity of the one open message and the
//! immutable snapshot of its displayable body a surface renders in the
//! reading pane.
//!
//! A mailbox-list row names a message or a thread; the reading pane always
//! reads one message, so its identity is message-shaped: [`MessageKey`] is the
//! payload of [`RowKey::Message`](crate::RowKey::Message) standing alone. The
//! snapshot carries display scalars and bodies only, no engine handle and no
//! host dependency; the pipeline that fetches and sanitises the body lives in
//! the application core, as the row projection does.

use engine_api::{AccountId, ProviderKey};
use serde::{Deserialize, Serialize};

use crate::RowKey;

/// The identity of the one message a reading snapshot holds the body of.
///
/// A provider key is unique only within its account, so the account is part
/// of the identity: the same provider key in two accounts names two messages,
/// and the pair is what a surface matches an open pane against to discard a
/// stale snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageKey {
    /// The account the message belongs to.
    pub account: AccountId,
    /// The message's provider key: its stable identity across mailbox moves,
    /// unique within the account.
    pub key: ProviderKey,
}

/// The error [`MessageKey::try_from`](TryFrom) returns: a thread row names a
/// whole conversation, and no member of it is THE one message a reading pane
/// reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAMessageRow;

impl core::fmt::Display for NotAMessageRow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("a thread row names a conversation, not one message")
    }
}

impl TryFrom<RowKey> for MessageKey {
    type Error = NotAMessageRow;

    /// Ties a selected list row to the message a reading pane opens. Total for a
    /// message row, whose payload is this key standing alone; a thread row is
    /// rejected rather than guessed at (which member would open is the caller's
    /// decision to make, not the conversion's).
    fn try_from(row: RowKey) -> Result<Self, Self::Error> {
        match row {
            RowKey::Message { account, key } => Ok(Self { account, key }),
            RowKey::Thread { .. } => Err(NotAMessageRow),
        }
    }
}

impl From<MessageKey> for RowKey {
    /// The reading key back into the row key it came from: the same pair, worn
    /// as a list row again.
    fn from(message: MessageKey) -> Self {
        Self::Message {
            account: message.account,
            key: message.key,
        }
    }
}

/// One downloadable attachment shown beneath the open message's body.
///
/// Metadata only: inline images ride inside the body's html as data URIs, and
/// saving an attachment is a verb of its own, carried out by the core through
/// this row's [`id`](Self::id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRow {
    /// The attachment's message-scoped id: what a save or a download names.
    pub id: u32,
    /// The suggested file name, from the part's headers.
    pub file_name: String,
    /// The part's media type, e.g. `application/pdf`.
    pub media_type: String,
    /// The decoded byte length.
    pub size: u64,
}

/// An immutable snapshot of the one open message's body, for a surface to
/// render in the reading pane.
///
/// `html` is already sanitised: the application core's body pipeline strips
/// the engine's raw `text/html` to a safe, inert subset and rewrites every
/// remote reference to a placeholder before the snapshot is published, so a
/// surface renders the string without anything remote loading. Both `html`
/// and `plain` absent means the message carries no text body at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingSnapshot {
    /// The identity of the message this body belongs to, so a surface can
    /// match the snapshot to the row it opened and ignore a stale one.
    pub message: MessageKey,
    /// The subject, empty if none.
    pub subject: String,
    /// The sender's display name, falling back to their email address when
    /// the header carried no name, and empty when there is no sender at all.
    ///
    /// One half of the reading header's sender; the other is
    /// [`from_address`](Self::from_address). Split, so a surface renders the
    /// pair without parsing a combined `Name <email>` string.
    pub from_name: String,
    /// The sender's bare email address, empty when there is no sender.
    ///
    /// Not for display beside the name; it is the identity an avatar is of.
    pub from_address: String,
    /// When the message was sent, as a whole-second UTC `…Z` instant
    /// (`2026-10-09T08:30:00Z`), empty when unknown: the same instant the
    /// list row for this message is ordered by. A surface localises it for
    /// display.
    pub sent_at: String,
    /// The sanitised HTML body, when the message carries an HTML part.
    ///
    /// Remote references inside it are placeholders, not loads; a surface
    /// renders it as inert markup and resolves the placeholders only when the
    /// reader opts in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// The plain-text body, when the message carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain: Option<String>,
    /// The message's downloadable attachments, empty when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentRow>,
    /// How many remote references the sanitised HTML carries, all of them
    /// blocked until the reader opts in: remote image sources and remote CSS
    /// loads (`url(...)` and `@import`) alike.
    ///
    /// `0` means the body has nothing remote to load, and a surface offers no
    /// toggle for it.
    pub remote_image_count: u32,
}

#[cfg(test)]
#[path = "reading_tests.rs"]
mod tests;
