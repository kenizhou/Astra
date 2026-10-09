// SPDX-License-Identifier: GPL-3.0-only
//! The compose-request view-model: the immutable snapshot of the composition a
//! reply or forward opens on, for a surface to hydrate the composer from.
//!
//! The core computes the whole prefill (recipients, subject, threading header,
//! quoted original) and hands it over as one thing, so a composer opens showing
//! exactly what the core derived and the host's own re-derivation can never
//! disagree with the send path. `in_reply_to` rides the snapshot so a draft saved
//! from a reply resumes with its threading intact.

use astra_composer::ComposerDocument;
use engine_api::AccountId;
use serde::{Deserialize, Serialize};

/// Which composition a compose request opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposeKind {
    /// A reply to the original's sender alone.
    Reply,
    /// A reply to the sender and every other recipient of the original.
    ReplyAll,
    /// A forward of the original to recipients the user names.
    Forward,
}

/// An immutable snapshot of the composition a reply or forward opens on.
///
/// `to` and `cc` are comma-joined bare addresses, the shape the composer's
/// recipient fields take: the prefill a user edits, not a send. `document` carries
/// the quoted original as its quote block, when the original had a body to quote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeRequestSnapshot {
    /// Which composition this is, so a surface renders the right verbs and the
    /// send path picks the right submit intent.
    pub kind: ComposeKind,
    /// The account to compose from: the original's own account, the one a reply
    /// is answered in unless the user picks another in the composer's From
    /// dropdown.
    pub from: AccountId,
    /// The `To` recipients, comma-joined; empty for a forward, whose recipients
    /// are the user's to name.
    pub to: String,
    /// The `Cc` recipients, comma-joined; non-empty only for a reply-all, which
    /// preserves the original's own Cc.
    pub cc: String,
    /// The subject, already prefixed (`Re:`/`Fwd:`) with the original's own
    /// prefix preserved rather than doubled.
    pub subject: String,
    /// The original's `Message-ID` for a reply to thread under, absent for a
    /// forward, which continues a conversation without answering a message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    /// The composer document the composer opens on: the quoted original below an
    /// empty paragraph, or no blocks when the original carried no body to quote.
    pub document: ComposerDocument,
}

#[cfg(test)]
#[path = "compose_request_tests.rs"]
mod tests;
