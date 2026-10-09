// SPDX-License-Identifier: GPL-3.0-only
//! Typed references: an [`AccountId`] bound to the key that names one thing it owns, so
//! the protocol can never carry a key without its owning account, nor pair a key with a
//! *different* account's id: the wrong-account routing class of bug (a provider key is
//! unique only *within* an account, so two accounts can mint the same one).
//!
//! A reference is built **once**, at the IPC boundary, from the very row the host acted
//! on; both halves travel together from there on.

use engine_api::{AccountId, ProviderKey};
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

/// A message of one account, identified by its owning account and provider key
/// **together**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRef {
    /// The account that owns the message (the row's account).
    pub account: AccountId,
    /// The message's provider key; unique only within `account`.
    pub key: ProviderKey,
}

impl MessageRef {
    /// Builds a message reference from a host's `account` id and provider `key` strings;
    /// the single construction point, at the IPC boundary. Returns `None` if either half
    /// is malformed, so the caller can drop the intent rather than risk routing to the
    /// wrong account or editing no message at all.
    #[must_use]
    pub fn from_parts(account: &str, key: String) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            key: ProviderKey::new(key).ok()?,
        })
    }
}

/// A conversation of one account, identified by its owning account and thread id
/// **together**: a thread id is projected per account, so it must travel with its
/// account for an action to route correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRef {
    /// The account that owns the conversation (the row's account).
    pub account: AccountId,
    /// The thread's id, as projected into the mailbox-list snapshot (a resolved thread
    /// id, or a lone message's own key). A plain identifier, not a provider key.
    pub thread_id: String,
}

impl ThreadRef {
    /// Builds a thread reference from a host's `account` id and `thread_id` strings; the
    /// single construction point, at the IPC boundary. Returns `None` if the `account` is
    /// malformed, so the caller can drop the intent rather than route to the wrong
    /// account. `thread_id` is a plain identifier and is not validated.
    #[must_use]
    pub fn from_parts(account: &str, thread_id: String) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            thread_id,
        })
    }
}

/// A mailbox-list **row** an action names: one message in the flat list, or one whole
/// conversation in the threaded list.
///
/// What a user acts on is a row, and the two view modes put different things on one, so
/// an action carries the row itself rather than a message key a client resolved for it.
/// Only the core can expand a conversation correctly: its members come from the store's
/// thread index, which holds messages a windowed list never listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowRef {
    /// One message: a flat row, or one message of an expanded conversation.
    Message(MessageRef),
    /// One conversation: a threaded row, standing for every message on the thread.
    Thread(ThreadRef),
}

impl RowRef {
    /// The account that owns the row; every write routes by this, so a row can never act
    /// in an account it was not drawn from.
    #[must_use]
    pub fn account(&self) -> &AccountId {
        match self {
            Self::Message(message) => &message.account,
            Self::Thread(thread) => &thread.account,
        }
    }
}

#[cfg(test)]
#[path = "reference_tests.rs"]
mod tests;
