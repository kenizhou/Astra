// SPDX-License-Identifier: GPL-3.0-only
//! The permission model a plugin's manifest declares and the host's fail-closed
//! default for each of them.
//!
//! Nothing here grants anything. [`default_approval`] answers
//! [`Approval::NeedsApproval`] for every permission the surface can name, so a
//! plugin starts entitled to nothing and a human decides every grant. The
//! exhaustive match (no wildcard arm) is deliberate: a permission added later
//! forces this table to answer for it before the crate compiles, which keeps the
//! fail-closed rule true by construction rather than by convention.

use serde::{Deserialize, Serialize};

/// One capability a plugin may hold. A manifest declares the set it wants; the
/// host, not the plugin, decides what it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read mailbox content: message bodies and metadata of stored mail.
    ReadMailbox,
    /// Transform outgoing mail at the compose seam; required by the
    /// `compose.transform` hook.
    TransformOutgoing,
    /// Decode incoming mail after fetch; required by the `message.decode` hook.
    DecodeIncoming,
}

impl Permission {
    /// Every permission of the current surface, in declaration order. The
    /// fail-closed table test walks this list, so extending it is part of adding
    /// a permission.
    pub const ALL: [Permission; 3] = [
        Permission::ReadMailbox,
        Permission::TransformOutgoing,
        Permission::DecodeIncoming,
    ];

    /// The name this permission carries in a manifest and on the wire.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            Permission::ReadMailbox => "read_mailbox",
            Permission::TransformOutgoing => "transform_outgoing",
            Permission::DecodeIncoming => "decode_incoming",
        }
    }
}

/// The host's answer to a plugin holding a permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    /// The capability waits for a human decision before it may be used.
    NeedsApproval,
    /// The capability is usable without a per-use prompt. No current permission
    /// defaults to this; it exists for a future host-side grant path a human has
    /// already decided on.
    Granted,
}

/// The fail-closed default for one permission: [`Approval::NeedsApproval`] today,
/// for every permission, by the exhaustive match below.
#[must_use]
pub fn default_approval(permission: Permission) -> Approval {
    match permission {
        Permission::ReadMailbox | Permission::TransformOutgoing | Permission::DecodeIncoming => {
            Approval::NeedsApproval
        }
    }
}

#[cfg(test)]
#[path = "permissions_tests.rs"]
mod tests;
