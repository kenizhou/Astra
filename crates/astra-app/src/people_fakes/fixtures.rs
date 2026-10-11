// SPDX-License-Identifier: GPL-3.0-only
//! The card and address-book builders every people test starts from: a
//! writable default book, formatted-only and structured name cards, bare
//! cards with neither name nor address, and the organisation values the
//! editor fields map.

use engine_api::{AddressBook, AddressBookId, ContactId};
use engine_core::contact::{
    ContactCard, ContactEmail, ContactName, ContactProperty, NameComponent, NameComponentKind,
    Organization, PropertyId, Title,
};
use engine_core::membership::Memberships;

/// A writable default address book keyed `key` called `name`.
pub(crate) fn book(key: &str, name: &str) -> AddressBook {
    let mut address_book = AddressBook::new(
        AddressBookId::try_from(key).unwrap(),
        name,
        engine_api::ContactSourceClass::Personal,
    );
    address_book.is_writable = true;
    address_book.is_default = true;
    address_book
}

/// A card with a formatted name only (the seeding rule's shape: the editor
/// holds the whole name in its given field) and the stated addresses, in
/// order.
pub(crate) fn named_card(id: &str, book: &str, name: &str, emails: &[&str]) -> ContactCard {
    let mut card = bare_card(id, book);
    card.name = Some(ContactName {
        full: Some(name.to_owned()),
        ..ContactName::default()
    });
    card.is_writable = true;
    for (at, address) in emails.iter().enumerate() {
        card.emails.insert(
            PropertyId::new(format!("e{}", at + 1)).unwrap(),
            ContactProperty::new(ContactEmail::new(*address)),
        );
    }
    card
}

/// A card with a structured name: a prefix, a given name, and a surname, the
/// shape whose components a structured rebuild must preserve.
pub(crate) fn structured_card(
    id: &str,
    book: &str,
    prefix: &str,
    given: &str,
    surname: &str,
    emails: &[&str],
) -> ContactCard {
    let mut card = bare_card(id, book);
    card.name = Some(ContactName {
        full: Some(format!("{prefix} {given} {surname}")),
        components: vec![
            NameComponent::new(NameComponentKind::Prefix, prefix),
            NameComponent::new(NameComponentKind::Given, given),
            NameComponent::new(NameComponentKind::Surname, surname),
        ],
        ..ContactName::default()
    });
    card.is_writable = true;
    for (at, address) in emails.iter().enumerate() {
        card.emails.insert(
            PropertyId::new(format!("e{}", at + 1)).unwrap(),
            ContactProperty::new(ContactEmail::new(*address)),
        );
    }
    card
}

/// A card with neither name nor address: the row whose words are the host's
/// to supply and whose monogram is the empty signal.
pub(crate) fn bare_card(id: &str, book: &str) -> ContactCard {
    ContactCard::new(
        ContactId::try_from(id).unwrap(),
        Memberships::of_one(AddressBookId::try_from(book).unwrap()),
    )
}

/// Adds one organisation affiliation and one title to a card.
pub(crate) fn with_organisation(
    mut card: ContactCard,
    organisation: &str,
    role: &str,
) -> ContactCard {
    card.organizations.insert(
        PropertyId::new("o1").unwrap(),
        ContactProperty::new(Organization {
            name: organisation.to_owned(),
            ..Organization::default()
        }),
    );
    card.titles.insert(
        PropertyId::new("t1").unwrap(),
        ContactProperty::new(Title {
            name: role.to_owned(),
            ..Title::default()
        }),
    );
    card
}
