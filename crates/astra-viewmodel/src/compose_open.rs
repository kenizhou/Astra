// SPDX-License-Identifier: GPL-3.0-only
//! The compose-open view-model: the prefill a `mailto:` link opens, the
//! additive shape the host hands a composer when the core's parser has read
//! the link. The twin of [`ComposeRequestSnapshot`](crate::ComposeRequestSnapshot)
//! for a link that answers no message: request-free, so the composition it
//! opens sends as plain mail exactly as a blank new message does.

use engine_api::AccountId;
use serde::{Deserialize, Serialize};

/// An immutable prefill for the composition a `mailto:` link opens.
///
/// The five fields the core's parser honours, nothing else: RFC 6068 lets a
/// link name any header, and the ones the parser refuses (`from` above all)
/// never reach this shape. `from` is the account the host resolved as the
/// default send account, so the composer opens from it without re-picking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailtoOpen {
    /// The default send account the composition opens from.
    pub from: AccountId,
    /// The `To` recipients, comma-separated (may be empty).
    pub to: String,
    /// The `Cc` recipients, comma-separated (may be empty).
    pub cc: String,
    /// The `Bcc` recipients, comma-separated (may be empty).
    pub bcc: String,
    /// The suggested subject (may be empty).
    pub subject: String,
    /// The suggested plain-text body, one paragraph per line (may be empty).
    pub body: String,
}

#[cfg(test)]
#[path = "compose_open_tests.rs"]
mod tests;
