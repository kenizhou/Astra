// SPDX-License-Identifier: GPL-3.0-only
//! The people half of the inbound protocol ([`PeopleIntent`]), which
//! [`Intent::People`](super::Intent::People) carries.
//!
//! Split from [`super::intent`] for the 500-line limit, following
//! [`CalendarIntent`](super::CalendarIntent). The family reads as a unit for
//! the same reason: the view verbs and the write verbs answer one surface,
//! and the reads delegate to the engine's own matching so a person beyond the
//! loaded page is still found.
//!
//! Two rules the engine forces and this family encodes:
//!
//! - **A write names a card, never a person alone.** A person is several
//!   cards across accounts, and writing a merged person's values back would
//!   file one account's details in another's book. A create names a writable
//!   address book bound to its account; an edit carries the person, the
//!   account, and the card together, the person there so a row held across a
//!   merge still resolves and the account and the card being what the write
//!   actually goes to.
//! - **Matching is the engine's.** The search intent hands the query down
//!   rather than filtering loaded rows, because only the engine sees every
//!   person, and a host re-filtering its page would silently unfind anyone
//!   the page cap had already cut off.

use engine_api::PersonId;
use serde::{Deserialize, Serialize};

use crate::reference::{AddressBookRef, CardRef};

/// A contact name as the editor states it: the two fields the form holds.
///
/// The given field carries the whole name when the card has no structured
/// one (the seeding rule), and the structured name is written only when
/// there is a surname: emitting components from a card that never had them
/// would file `Ada Lovelace` as a first name with no surname in every other
/// client the user owns, on a save that changed nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactNameInput {
    /// The given-name field: the first name, or the whole name when the card
    /// carries no structured one.
    pub given: String,
    /// The surname, when the form holds one. Absent, the name is written as
    /// a formatted one alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surname: Option<String>,
}

/// A new contact as the create form states it: the editor's fields, in the
/// engine's own vocabulary once mapped.
///
/// A create must be findable again (a name, an organisation, or an address);
/// the core refuses one that names nothing to file the card under, and that
/// refusal is the `Invalid` write state, not an error here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactDraftInput {
    /// The name, if the form holds one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ContactNameInput>,
    /// The organisation name, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organisation: Option<String>,
    /// The role or title, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The email addresses, in card order: the first is the person's primary
    /// address, the one the avatar and the list row key on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<String>,
    /// The phone numbers, in card order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phones: Vec<String>,
}

/// What an edit changes about one card: only the fields that changed.
///
/// Every field an edit names is replaced, and a replacement built from a form
/// loses whatever the form cannot show (an address's contexts, an
/// organisation's departments), so a changed field is rebuilt on top of the
/// property the card already carries and an unchanged one is left out
/// entirely. `None` leaves a field alone; the empty string clears the flat
/// ones; the empty list clears the lists. An edit that changed nothing sends
/// nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactPatchInput {
    /// The name, rebuilt on top of the card's own when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ContactNameInput>,
    /// The organisation name; `Some("")` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organisation: Option<String>,
    /// The role or title; `Some("")` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The email addresses as the new whole list, in their new order;
    /// `Some([])` clears them all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emails: Option<Vec<String>>,
    /// The phone numbers as the new whole list; `Some([])` clears them all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phones: Option<Vec<String>>,
}

/// What the people surface asks the runtime to do.
///
/// The reads and writes share one family because they answer one surface:
/// the view verbs position the list and the detail, the write verbs change
/// the cards the list renders, and every write's outcome the snapshot's
/// write states carry. No delete: the ported contract does not offer it, the
/// gap the spec records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PeopleIntent {
    /// Sync the accounts' contact sources and refresh the people snapshot.
    Refresh,
    /// Match the query in the engine, over name, email, phone, organisation,
    /// and title, and publish the matched rows as the list. An empty query is
    /// the unfiltered list.
    Search {
        /// The text to match, handed to the engine verbatim.
        query: String,
    },
    /// Open one person's detail. The engine's person id, which still resolves
    /// after a background merge retires it, so a row a host is still holding
    /// opens without refreshing the list first.
    OpenPerson {
        /// The person to open.
        person: PersonId,
    },
    /// Shut the detail: the surface goes back to the list alone.
    ClosePerson,
    /// Create a contact in a writable address book. The book reference binds
    /// the book to the account whose provider serves it, because a book id is
    /// unique only within its account.
    CreateContact {
        /// The writable destination, bound to its owning account.
        book: AddressBookRef,
        /// The new card's values, as the create form stated them.
        draft: ContactDraftInput,
    },
    /// Edit one source card: the core re-reads the card the reference names,
    /// rebuilds each changed field on top of the property the card carries,
    /// and submits the result through the engine's patch verb, so properties
    /// this model does not carry survive.
    UpdateContact {
        /// The card to edit: the person, the account, and the card together.
        card: CardRef,
        /// What the edit changes.
        patch: ContactPatchInput,
    },
}
