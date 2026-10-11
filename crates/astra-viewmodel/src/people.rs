// SPDX-License-Identifier: GPL-3.0-only
//! The people view-model: an immutable snapshot a host pulls after a
//! `people` signal and renders as the unified contacts list and the open
//! person's detail.
//!
//! Pure projection over the engine's unified people state, like the calendar
//! beside it: the join, the ordering, and the matching live in the engine, the
//! rebuild in the app derives the presentation facts the engine does not
//! carry (the section letter, the monogram, the account count) once, and
//! every surface shares this one definition of what is on screen. A host
//! never recounts, refilters, or re-derives: the rows arrive already joined,
//! already ordered, already sectioned.

use engine_api::{AccountId, AddressBookId, ContactId, PersonId};
use serde::{Deserialize, Serialize};

/// An immutable people snapshot for a surface to render: the unified rows,
/// the open person's detail, the write states, and the create targets.
///
/// Empty before any people state exists: a cold host renders an empty list,
/// not a zeroed one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeopleSnapshot {
    /// The unified people the current query matched, in the engine's page
    /// order: what the list lays out, one row per person whatever number of
    /// cards and accounts were joined into them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<PersonRow>,
    /// The open person's detail, or nothing when the list is showing with
    /// nobody open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<PersonDetail>,
    /// The accounts with a contact write in flight, unconfirmed, or refused
    /// as invalid: what a surface draws its write status from. No row means
    /// nothing is being written and nothing needs confirming.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_states: Vec<PeopleWriteRow>,
    /// The writable address books a create may land in, per account: what the
    /// create affordance offers. Empty crosses no key, and a surface hides
    /// the affordance entirely rather than offering a save with nowhere to
    /// go.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub create_targets: Vec<CreateTargetRow>,
}

/// One unified person, as the list renders them.
///
/// The disclosure facts the row carries are settled facts, not inputs for a
/// host to compute with: the account count is counted from the engine's
/// sources (distinct accounts, never source cards), the section letter is
/// folded from the display name, and the monogram is derived from the name
/// and the primary address. A host that recomputed any of them could
/// disagree with the detail screen beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonRow {
    /// The engine's person id: what an open intent names. Stable across
    /// restarts, and still resolving after a background merge retires it.
    pub person: PersonId,
    /// The person's display name, empty when the sources carry none: the
    /// signal for the host to supply its own placeholder words, never a
    /// placeholder invented here.
    pub display_name: String,
    /// The monogram the row's avatar draws.
    pub monogram: Monogram,
    /// How many distinct accounts the person's source cards span. Absent for
    /// one: `In 1 accounts` is noise on every ordinary contact, so the count
    /// crosses no key until there is a merge to disclose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_count: Option<u32>,
    /// The section letter the row files under: an uppercased letter, or `#`
    /// for anything that does not start with one. The fold is the rebuild's
    /// (the engine orders by raw lowercased code points and does not fold),
    /// so the rebuild must lay the rows out by its own fold order, not the
    /// page's, or the sections and the order disagree.
    pub section: String,
    /// The person's primary address, the first email their sources carry:
    /// what the avatar's colour is keyed on and a tap-to-mail affordance
    /// offers. Absent when the person has no address at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_address: Option<String>,
}

/// What a person's avatar draws before any photo arrives: the letters and the
/// fill they sit on, derived once so the same person cannot be `AL` on one
/// surface and `A` on another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Monogram {
    /// The initials: the first character of the first and last words of the
    /// name, uppercased; the address's first character when there is no name;
    /// empty when there is neither, the signal for the host to draw its own
    /// person glyph.
    pub initials: String,
    /// The fill behind the letters, a CSS colour from the shared palette,
    /// keyed on the lowercased primary address so it is stable per person.
    /// A free string on the wire; the palette itself is finite and the
    /// rebuild only ever emits a slot from it.
    pub colour: String,
}

/// The open person's detail: the sources it was joined from, the values with
/// their provenance, and the cards an edit may target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonDetail {
    /// The person the detail is of, the same id the open intent named.
    pub person: PersonId,
    /// The person's display name, the same signal the row carries.
    pub display_name: String,
    /// Every source card the person was assembled from, ordered by account
    /// then card: what the detail names under its Also-in disclosure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<PersonSourceRow>,
    /// The person's values, each with the source cards that supplied it: what
    /// the detail tags per value (this address is in your work account, that
    /// one in both).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<PersonValueRow>,
    /// The source cards an edit may target, each carrying the person, the
    /// account, and the card the write names together: what the editor offers
    /// as its choice, and what an update intent's reference is built from.
    /// Empty when the person cannot be edited anywhere.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editable_cards: Vec<EditableCardRow>,
}

/// One provider card a person was assembled from: a row of the detail's
/// source list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonSourceRow {
    /// The account holding the card: the disclosure names accounts, because
    /// that is the boundary the user understands (work beside personal).
    pub account: AccountId,
    /// The card's provider id, unique only within `account`.
    pub card: ContactId,
    /// Whether this exact source record can be edited at its origin: a
    /// directory or a shared book the account may only read is a place
    /// contacts come from, never one an edit may write back to.
    pub writable: bool,
}

/// One of a person's values with its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonValueRow {
    /// Which kind of value this is. Crosses as a snake_case word.
    pub kind: PersonValueKind,
    /// The value itself, as the engine normalised it.
    pub value: String,
    /// The source cards that supplied the value, empty when the engine
    /// recorded none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<PersonSourceRow>,
}

/// The kinds of value a person's detail carries. Crosses as a snake_case
/// word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonValueKind {
    /// A name the sources carry.
    Name,
    /// An email address.
    Email,
    /// A phone number.
    Phone,
    /// An organisation name.
    Organisation,
    /// A title or role.
    Title,
}

/// One card an edit may target, carrying the person beside the account and
/// the card: the triple a contact write names, so a row a host is still
/// holding after a merge still resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditableCardRow {
    /// The person the card belongs to: the merge-survival half of the
    /// triple, resolving through the engine's alias table after a merge.
    pub person: PersonId,
    /// The account whose provider holds the card: where the write routes.
    pub account: AccountId,
    /// The card's provider id, unique only within `account`.
    pub card: ContactId,
}

/// One account's contact write, as the surface's write status shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeopleWriteRow {
    /// The account whose write the row reports on.
    pub account: AccountId,
    /// Where the write has got to.
    pub state: PeopleWriteState,
}

/// Where a contact write has got to. Crosses as a snake_case word.
///
/// The calendar's two states plus the one a contacts form adds: a refused
/// form is neither settling nor unconfirmed, and the sentence it gets is a
/// different one, under the form the user is still looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeopleWriteState {
    /// A contact write is in flight.
    Writing,
    /// The last write did not settle cleanly: the engine refused it outright,
    /// or it landed with only the local confirmation missing. The next step
    /// is the same either way: a re-read resolves the row and shows whether
    /// the change is there to keep or needs making again.
    FailedWithRetry,
    /// The core refused the form before sending anything: something to
    /// correct and retry, not an unconfirmed save. A surface states it under
    /// the form, never beside the list.
    Invalid,
}

/// One writable address book a create may land in, per account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTargetRow {
    /// The account whose provider serves the book: where the created card
    /// routes.
    pub account: AccountId,
    /// The book's provider id, unique only within `account`.
    pub book: AddressBookId,
    /// The book's display name, for the create form's choice of destination.
    pub name: String,
    /// Whether the provider files new contacts here unless the user picks
    /// another book: the form's preselection.
    pub is_default: bool,
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
