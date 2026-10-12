// SPDX-License-Identifier: GPL-3.0-only
//! Typed references: an [`AccountId`] bound to the key that names one thing it owns, so
//! the protocol can never carry a key without its owning account, nor pair a key with a
//! *different* account's id: the wrong-account routing class of bug (a provider key is
//! unique only *within* an account, so two accounts can mint the same one).
//!
//! A reference is built **once**, at the IPC boundary, from the very row the host acted
//! on; both halves travel together from there on.

use engine_api::{
    AccountId, AddressBookId, CalendarId, ContactId, PendingOpId, PersonId, ProviderKey,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

/// A calendar of one account, identified by its owning account and calendar key
/// **together**.
///
/// A calendar key is unique only *within* an account (two accounts can each hold
/// a calendar keyed `default`), so every calendar an intent names or a snapshot
/// row carries travels with the account that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarRef {
    /// The account that owns the calendar (the calendar row's account).
    pub account: AccountId,
    /// The calendar's key, as projected into the snapshot's calendar rows; unique
    /// only within `account`.
    pub key: CalendarId,
}

impl CalendarRef {
    /// Builds a calendar reference from a host's `account` id and calendar `key`
    /// strings; the single construction point, at the IPC boundary. Returns `None`
    /// if either half is malformed, so the caller can drop the intent rather than
    /// route a write into the wrong account's calendar.
    #[must_use]
    pub fn from_parts(account: &str, key: String) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            key: CalendarId::new(ProviderKey::new(key).ok()?),
        })
    }
}

/// An event of one account, identified by its owning account and provider key
/// **together**.
///
/// The key is the **master** event's: an occurrence of a recurring series is
/// not a stored object of its own, so the occurrence an edit or delete means
/// is named beside the key (by its original start), never by a synthetic key
/// for the instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRef {
    /// The account that owns the event (the occurrence row's account).
    pub account: AccountId,
    /// The master event's provider key; unique only within `account`.
    pub key: ProviderKey,
}

impl EventRef {
    /// Builds an event reference from a host's `account` id and provider `key`
    /// strings; the single construction point, at the IPC boundary. Returns
    /// `None` if either half is malformed, so the caller can drop the intent
    /// rather than risk routing to the wrong account or editing no event at
    /// all.
    #[must_use]
    pub fn from_parts(account: &str, key: String) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            key: ProviderKey::new(key).ok()?,
        })
    }
}

/// An address book of one account, identified by its owning account and book
/// id **together**.
///
/// A book id is unique only *within* an account (every provider calls its
/// default book `default`), so a create must name the two together or risk
/// landing a contact in another account's book of the same name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressBookRef {
    /// The account whose provider serves the book.
    pub account: AccountId,
    /// The book's provider id; unique only within `account`.
    pub book: AddressBookId,
}

impl AddressBookRef {
    /// Builds an address-book reference from a host's `account` id and book
    /// id strings; the single construction point, at the IPC boundary.
    /// Returns `None` if either half is malformed, so the caller can drop the
    /// intent rather than create into the wrong account's book.
    #[must_use]
    pub fn from_parts(account: &str, book: String) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            book: AddressBookId::new(ProviderKey::new(book).ok()?),
        })
    }
}

/// A contact card an edit names, identified by the **person, the account, and
/// the card together**.
///
/// The account and the card are what the write actually goes to; the person
/// rides for merge survival, so a row a host is still holding after a
/// background merge still resolves to the cards that survive it. A person
/// alone never names a write: one person is several cards, and writing a
/// merged person's values back would file one account's details in another's
/// book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardRef {
    /// The person the card belongs to, resolving through the engine's alias
    /// table after a merge retires the id.
    pub person: PersonId,
    /// The account whose provider holds the card.
    pub account: AccountId,
    /// The card's provider id; unique only within `account`.
    pub card: ContactId,
}

impl CardRef {
    /// Builds a card reference from a host's person number, `account` id, and
    /// card id strings; the single construction point, at the IPC boundary.
    /// Returns `None` if any half is malformed (a zero person is the reserved
    /// sentinel), so the caller can drop the intent rather than edit some
    /// other account's card of the same id.
    #[must_use]
    pub fn from_parts(person: u64, account: &str, card: String) -> Option<Self> {
        Some(Self {
            person: PersonId::new(person).ok()?,
            account: AccountId::try_from(account).ok()?,
            card: ContactId::new(ProviderKey::new(card).ok()?),
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

/// A **queued** send, identified by its owning account and outbox op id
/// **together**.
///
/// The outbox is account-scoped, so an op id means nothing without the account
/// whose queue it belongs to, exactly as a folder key means nothing without its
/// account. Unlike every other reference here it names a message that does
/// **not** exist on any server yet: the identity is the durable op, not a
/// provider key.
///
/// The op id crosses the wire as the bare number the store assigned it,
/// because [`PendingOpId`] carries no serialisation of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedRef {
    /// The account whose outbox holds the send (the row's account).
    pub account: AccountId,
    /// The durable op's id, as projected into the sending snapshot's rows.
    pub op: PendingOpId,
}

impl QueuedRef {
    /// Builds a queued-send reference from a host's `account` id and outbox op
    /// id; the single construction point, at the IPC boundary. Returns `None`
    /// for a malformed account id, so the caller can drop the intent rather
    /// than route an action into the wrong account's queue.
    ///
    /// The op id needs no validation of its own: it is a store-assigned
    /// integer, and one that names nothing is refused by the store as
    /// `OpRejection::Unknown` rather than guessed at here.
    #[must_use]
    pub fn from_parts(account: &str, op: u64) -> Option<Self> {
        Some(Self {
            account: AccountId::try_from(account).ok()?,
            op: PendingOpId::new(op),
        })
    }
}

impl Serialize for QueuedRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        /// The reference's two halves as the wire carries them.
        #[derive(Serialize)]
        struct Wire<'a> {
            account: &'a AccountId,
            op: u64,
        }
        Wire {
            account: &self.account,
            op: self.op.get(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for QueuedRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// The reference's two halves as the wire carries them.
        #[derive(Deserialize)]
        struct Wire {
            account: AccountId,
            op: u64,
        }
        let Wire { account, op } = Wire::deserialize(deserializer)?;
        Ok(Self {
            account,
            op: PendingOpId::new(op),
        })
    }
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
