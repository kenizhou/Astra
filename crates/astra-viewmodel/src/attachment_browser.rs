// SPDX-License-Identifier: GPL-3.0-only
//! The attachment browser's view-model: the flattened attachment rows across
//! mail beside the coverage report the scan answers honestly with, and the
//! filter a browse narrows by. A command's answer, never a Surface: the
//! browser recomputes per open, so there is no published state to signal.

use engine_api::AccountId;
use serde::{Deserialize, Serialize};

use crate::MessageKey;

/// What one browse narrows by: a sender and a media-type prefix, each absent
/// when the browse runs unfiltered. Session state the surface holds, never
/// persisted: a filter is a property of one browse, not a preference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentBrowserFilter {
    /// The sender's bare address, matched exactly and case-insensitively;
    /// `None` admits every sender.
    pub sender: Option<String>,
    /// The media-type prefix a part must carry (`image/`, `application/pdf`);
    /// `None` admits every media type.
    pub mime_prefix: Option<String>,
}

/// One downloadable attachment beside the message it came from: the part
/// metadata a save names, the source reference an open dispatches, and the
/// source row's display scalars the group header draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentBrowserEntry {
    /// The attachment's message-scoped id: what a save names, the same id the
    /// reading snapshot's attachment row carries.
    pub id: u32,
    /// The suggested file name, from the part's headers.
    pub file_name: String,
    /// The part's media type, e.g. `application/pdf`.
    pub media_type: String,
    /// The decoded byte length.
    pub size: u64,
    /// The message the part belongs to: the account and provider key a save
    /// or an open names it by.
    pub source: MessageKey,
    /// The source message's subject, empty if none.
    pub subject: String,
    /// The sender's display name, falling back to their address when the
    /// header carried no name, and empty when there is no sender at all.
    pub from_name: String,
    /// The sender's bare email address, empty when there is no sender.
    pub from_address: String,
    /// When the source message was sent, as a whole-second UTC `…Z` instant,
    /// empty when unknown: the same instant the list row is ordered by.
    pub sent_at: String,
}

/// How far one browse looked, stated honestly: the messages the scan
/// considered, the ones whose parts it listed, the ones whose read refused,
/// and the accounts whose scan failed outright. The footer renders these
/// beside the scan window so a bounded answer never claims the whole mailbox.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentBrowserCoverage {
    /// The messages the scan considered across every account: the
    /// has_attachment hits the store resolved, before any filter.
    pub scanned: u32,
    /// The messages whose attachment parts the scan listed.
    pub indexed: u32,
    /// The messages whose parts could not be read: the source fetch refused
    /// or the message no longer resolved. Best-effort skips, never fatal.
    pub skipped: u32,
    /// The accounts whose scan refused outright, each named by its id.
    pub failed_accounts: Vec<AccountId>,
    /// The per-account scan window: how many messages one account's query
    /// may return, stated beside the answer so the horizon is visible.
    pub scan_limit: u32,
}

/// One browse's whole answer: the flattened rows newest first beside the
/// coverage report. Recomputed per open; nothing persists between browses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentBrowserView {
    /// The attachment rows, newest source message first, parts in the order
    /// the message carries them.
    pub entries: Vec<AttachmentBrowserEntry>,
    /// How far the browse looked.
    pub coverage: AttachmentBrowserCoverage,
}
