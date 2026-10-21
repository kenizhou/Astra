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
//!
//! The seven tiers are cumulative in Outlook's sense, with one deliberate
//! split: [`Permission::ComposeEdit`] and [`Permission::Send`] are separate
//! powers, so a plugin handed the composer's pen never gains the power to
//! send with it. `restricted` is not a variant: it is the absence of any
//! grant, the state every plugin starts in.

use serde::{Deserialize, Serialize};

/// One capability a plugin may hold. A manifest declares the set it wants; the
/// host, not the plugin, decides what it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read the current item only: the message the user is looking at, its
    /// headers, body, and attachments. The host mints handles for that item
    /// alone.
    ReadItem,
    /// Read the whole mailbox (folders, lists, search) and mutate item state
    /// (read flags, flags, archive, trash) through the object model's routed
    /// methods.
    ReadMailbox,
    /// Write through the composer: document edits, recipients, options, and
    /// the `compose.transform` seam. Never includes the power to send.
    ComposeEdit,
    /// Send-class power: `message.send()` and the `before_send` gate. Per-use
    /// approval for the first send of each composition unless the user
    /// pre-approves the plugin for that account.
    Send,
    /// Administrative power: key management surfaces, event subscriptions,
    /// and any auto-activated `before_send`. Requires an explicit admin or
    /// enterprise grant.
    Admin,
    /// Host-mediated inference only: the AI methods of the object model,
    /// always budget-checked and audited by the host. Implied by no other
    /// tier, and never a provider key or a direct model call.
    #[serde(rename = "ai.infer")]
    AiInfer,
}

impl Permission {
    /// Every permission of the current surface, in declaration order. The
    /// fail-closed table test walks this list, so extending it is part of adding
    /// a permission.
    pub const ALL: [Permission; 6] = [
        Permission::ReadItem,
        Permission::ReadMailbox,
        Permission::ComposeEdit,
        Permission::Send,
        Permission::Admin,
        Permission::AiInfer,
    ];

    /// The name this permission carries in a manifest and on the wire.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            Permission::ReadItem => "read_item",
            Permission::ReadMailbox => "read_mailbox",
            Permission::ComposeEdit => "compose_edit",
            Permission::Send => "send",
            Permission::Admin => "admin",
            Permission::AiInfer => "ai.infer",
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
        Permission::ReadItem
        | Permission::ReadMailbox
        | Permission::ComposeEdit
        | Permission::Send
        | Permission::Admin
        | Permission::AiInfer => Approval::NeedsApproval,
    }
}

#[cfg(test)]
#[path = "permissions_tests.rs"]
mod tests;
