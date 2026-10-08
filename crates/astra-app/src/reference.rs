// SPDX-License-Identifier: GPL-3.0-only
//! Typed references: an [`AccountId`] bound to the key that names one thing it owns, so
//! the protocol can never carry a key without its owning account, nor pair a key with a
//! *different* account's id: the wrong-account routing class of bug (a provider key is
//! unique only *within* an account, so two accounts can mint the same one).
//!
//! A reference is built **once**, at the IPC boundary, from the very row the host acted
//! on; both halves travel together from there on.

use engine_api::AccountId;
use serde::{Deserialize, Serialize};

/// A folder of one account, identified by its owning account and folder key **together**.
///
/// A folder key is unique only *within* an account (every provider calls its inbox
/// `inbox`), so a pane holding every account's tree holds several rows carrying the same
/// key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderRef {
    /// The account that owns the folder (the pane row's account).
    pub account: AccountId,
    /// The folder's key, as projected into the snapshot's folder rows; unique only within
    /// `account`. A plain identifier, not a provider key.
    pub key: String,
}

impl FolderRef {
    /// Builds a folder reference from a host's `account` id and folder `key` strings; the
    /// single construction point, at the IPC boundary. Returns `None` if the `account` is
    /// malformed, so the caller can drop the intent rather than open some other account's
    /// folder of the same name. `key` is a plain identifier and is not validated.
    #[must_use]
    pub fn from_parts(account: &str, key: String) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            key,
        })
    }
}

#[cfg(test)]
#[path = "reference_tests.rs"]
mod tests;
