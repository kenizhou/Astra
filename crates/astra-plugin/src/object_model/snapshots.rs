// SPDX-License-Identifier: GPL-3.0-only
//! The snapshot structs: the immutable read shapes the host answers object
//! calls with. A snapshot is a point-in-time copy; the plugin never holds a
//! live view, and the host republishes fresh snapshots through its own event
//! streams when the underlying state moves.
//!
//! The shapes here are the slice's core three plus the blob reference; the
//! tree widens as later slices route their nodes.

use serde::{Deserialize, Serialize};

/// An immutable read of one message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageSnapshot {
    /// The message's subject.
    pub subject: String,
    /// The sender's address.
    pub from: String,
    /// The To recipients' addresses.
    pub to: Vec<String>,
    /// The Cc recipients' addresses.
    pub cc: Vec<String>,
    /// When the message was sent, RFC 3339, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    /// Whether the message is unread.
    pub unread: bool,
    /// Whether the message is flagged.
    pub flagged: bool,
    /// Whether the message carries attachments.
    pub has_attachments: bool,
    /// The plain-text body, when the call's scope includes the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
}

/// An immutable read of one folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FolderSnapshot {
    /// The folder's display name.
    pub name: String,
    /// The folder's role, for example `inbox` or `trash`.
    pub role: String,
    /// The number of unread messages in the folder.
    pub unread_count: u64,
}

/// An immutable read of one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AccountSnapshot {
    /// The account's display name.
    pub display_name: String,
    /// The account's primary address.
    pub email: String,
}

/// A reference to a large payload in the host-managed blob cache. Blobs cross
/// the boundary by reference, never by copy: the plugin reads and writes them
/// through host-mediated blob calls, and the cache enforces a per-plugin
/// quota with LRU eviction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BlobRef {
    /// The blob's id in the cache.
    pub blob_id: String,
    /// The blob's size in bytes.
    pub size: u64,
    /// The blob's SHA-256, lowercase hex, so a reader can verify what it got.
    pub sha256: String,
}

#[cfg(test)]
#[path = "snapshots_tests.rs"]
mod tests;
