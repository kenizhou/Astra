// SPDX-License-Identifier: GPL-3.0-only
//! The compose half of the inbound protocol ([`ComposeIntent`]), which
//! [`Intent::Compose`] carries.
//!
//! Split from [`super::intent`] to keep each file under the 500-line limit,
//! following [`OutboxIntent`](super::OutboxIntent). Compose reads as a unit for
//! the reason the outbox does: three verbs that mean nothing anywhere else in
//! the app, because only they answer one message the store already holds. The
//! prefill each verb computes is the [compose-request
//! surface's](super::Surface::ComposeRequest) to publish; what the composed
//! message eventually does is the submit intents' business, not this family's.

use serde::{Deserialize, Serialize};

use crate::reference::MessageRef;

/// What the reading pane asks the runtime to compose from one message.
///
/// Every variant names the original by [`MessageRef`]: the account and provider
/// key bound together, so the header reads the prefill computes from always
/// resolve against the account that actually holds the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ComposeIntent {
    /// Compose a reply to the original's sender alone: the prefill puts the
    /// sender in `To` and leaves `Cc` to the user.
    Reply {
        /// The message to reply to, bound to its owning account.
        message: MessageRef,
    },
    /// Compose a reply to the sender and every other recipient: the prefill
    /// preserves the original's layout, `To` beside `To` and `Cc` beside `Cc`,
    /// with the account's own identity left out of both.
    ReplyAll {
        /// The message to reply to, bound to its owning account.
        message: MessageRef,
    },
    /// Compose a forward of the original: the prefill names no recipient (they
    /// are the user's to choose) and quotes the original under a forwarded
    /// marker rather than a reply's attribution. `carry` names the original's
    /// downloadable parts (the ids the reading snapshot's attachment rows
    /// carry) the forward opens holding as removable attachments; the parts
    /// ride the send as regular files, and a row the composer removed simply
    /// does not ride.
    Forward {
        /// The message to forward, bound to its owning account.
        message: MessageRef,
        /// The original's part ids to pre-attach; empty crosses as the bare
        /// forward the wire has always carried.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        carry: Vec<u32>,
    },
}
