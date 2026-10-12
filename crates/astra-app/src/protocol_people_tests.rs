// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the people half of the inbound protocol: the exact JSON wire shape every
//! [`PeopleIntent`](super::PeopleIntent) crosses the IPC in, that each reads back
//! identical, and the additive `people` surface word.

use super::{
    ContactDraftInput, ContactNameInput, ContactPatchInput, Intent, PeopleIntent, Surface,
};
use crate::reference::{AddressBookRef, CardRef};

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

/// A writable address book of the work account, the way the create literal names one.
fn book() -> AddressBookRef {
    AddressBookRef::from_parts("work", "/dav/card/work/default/".to_owned()).unwrap()
}

/// The work account's Ada card, the way the update literal names one: the
/// person, the account, and the card together.
fn card() -> CardRef {
    CardRef::from_parts(7, "work", "/dav/card/work/default/ada.vcf".to_owned()).unwrap()
}

#[test]
fn refresh_and_close_cross_as_bare_words() {
    round_trips(
        &Intent::People(PeopleIntent::Refresh),
        r#"{"people":"refresh"}"#,
    );
    round_trips(
        &Intent::People(PeopleIntent::ClosePerson),
        r#"{"people":"close_person"}"#,
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<Intent>(r#"{"people":"Refresh"}"#).is_err());
}

#[test]
fn search_and_open_person_carry_their_single_fact() {
    round_trips(
        &Intent::People(PeopleIntent::Search {
            query: "ada".to_owned(),
        }),
        r#"{"people":{"search":{"query":"ada"}}}"#,
    );
    // An empty query is the unfiltered list, not a refusal: the engine matches
    // everything when the needle is blank.
    round_trips(
        &Intent::People(PeopleIntent::Search {
            query: String::new(),
        }),
        r#"{"people":{"search":{"query":""}}}"#,
    );
    // The person id crosses as the bare number the engine assigned.
    round_trips(
        &Intent::People(PeopleIntent::OpenPerson {
            person: engine_api::PersonId::new(7).unwrap(),
        }),
        r#"{"people":{"open_person":{"person":7}}}"#,
    );
}

#[test]
fn a_create_names_the_book_and_carries_the_editor_s_fields() {
    // The full create: a structured name, the affiliation, and both lists in
    // card order, the first email the primary address.
    round_trips(
        &Intent::People(PeopleIntent::CreateContact {
            book: book(),
            draft: ContactDraftInput {
                name: Some(ContactNameInput {
                    given: "Ada".to_owned(),
                    surname: Some("Lovelace".to_owned()),
                }),
                organisation: Some("Astra".to_owned()),
                role: Some("Engineer".to_owned()),
                emails: vec![
                    "ada@example.test".to_owned(),
                    "ada.lovelace@example.test".to_owned(),
                ],
                phones: vec!["+31 20 555 0100".to_owned()],
            },
        }),
        concat!(
            r#"{"people":{"create_contact":{"book":{"account":"work","#,
            r#""book":"/dav/card/work/default/"},"#,
            r#""draft":{"name":{"given":"Ada","surname":"Lovelace"},"#,
            r#""organisation":"Astra","role":"Engineer","#,
            r#""emails":["ada@example.test","ada.lovelace@example.test"],"#,
            r#""phones":["+31 20 555 0100"]}}}}"#
        ),
    );
    // The minimal create: an organisation alone is findable again, and every
    // absent field crosses no key at all.
    round_trips(
        &Intent::People(PeopleIntent::CreateContact {
            book: book(),
            draft: ContactDraftInput {
                name: None,
                organisation: Some("Astra".to_owned()),
                role: None,
                emails: Vec::new(),
                phones: Vec::new(),
            },
        }),
        concat!(
            r#"{"people":{"create_contact":{"book":{"account":"work","#,
            r#""book":"/dav/card/work/default/"},"draft":{"organisation":"Astra"}}}}"#
        ),
    );
    // A name with no surname crosses no surname key: the structured name is
    // written only when there is one, so the whole name stays a formatted one.
    round_trips(
        &Intent::People(PeopleIntent::CreateContact {
            book: book(),
            draft: ContactDraftInput {
                name: Some(ContactNameInput {
                    given: "Ada Lovelace".to_owned(),
                    surname: None,
                }),
                organisation: None,
                role: None,
                emails: vec!["ada@example.test".to_owned()],
                phones: Vec::new(),
            },
        }),
        concat!(
            r#"{"people":{"create_contact":{"book":{"account":"work","#,
            r#""book":"/dav/card/work/default/"},"#,
            r#""draft":{"name":{"given":"Ada Lovelace"},"emails":["ada@example.test"]}}}}"#
        ),
    );
}

#[test]
fn an_update_names_the_card_and_patches_only_what_changed() {
    // The minimal edit changes one field; every field it does not touch
    // crosses no key, so the core leaves those properties exactly as the
    // card carries them.
    round_trips(
        &Intent::People(PeopleIntent::UpdateContact {
            card: card(),
            patch: ContactPatchInput {
                name: None,
                organisation: Some("Astra".to_owned()),
                role: None,
                emails: None,
                phones: None,
            },
        }),
        concat!(
            r#"{"people":{"update_contact":{"card":{"person":7,"account":"work","#,
            r#""card":"/dav/card/work/default/ada.vcf"},"patch":{"organisation":"Astra"}}}}"#
        ),
    );
    // The full patch: the name rewritten as structured, both lists replaced
    // in their new order.
    round_trips(
        &Intent::People(PeopleIntent::UpdateContact {
            card: card(),
            patch: ContactPatchInput {
                name: Some(ContactNameInput {
                    given: "Ada".to_owned(),
                    surname: Some("Byron".to_owned()),
                }),
                organisation: Some("Astra".to_owned()),
                role: Some("Principal Engineer".to_owned()),
                emails: Some(vec!["ada@example.test".to_owned()]),
                phones: Some(vec![
                    "+31 20 555 0100".to_owned(),
                    "+31 20 555 0199".to_owned(),
                ]),
            },
        }),
        concat!(
            r#"{"people":{"update_contact":{"card":{"person":7,"account":"work","#,
            r#""card":"/dav/card/work/default/ada.vcf"},"#,
            r#""patch":{"name":{"given":"Ada","surname":"Byron"},"#,
            r#""organisation":"Astra","role":"Principal Engineer","#,
            r#""emails":["ada@example.test"],"#,
            r#""phones":["+31 20 555 0100","+31 20 555 0199"]}}}}"#
        ),
    );
    // A cleared list is the empty list, not an absent key: clearing the
    // phones removes them all, while leaving the key out entirely would leave
    // them alone.
    round_trips(
        &Intent::People(PeopleIntent::UpdateContact {
            card: card(),
            patch: ContactPatchInput {
                name: None,
                organisation: None,
                role: Some(String::new()),
                emails: None,
                phones: Some(Vec::new()),
            },
        }),
        concat!(
            r#"{"people":{"update_contact":{"card":{"person":7,"account":"work","#,
            r#""card":"/dav/card/work/default/ada.vcf"},"patch":{"role":"","phones":[]}}}}"#
        ),
    );
    // A patch that names nothing changes nothing: it crosses an empty object
    // rather than being refused on the wire, and the core sends nothing.
    round_trips(
        &Intent::People(PeopleIntent::UpdateContact {
            card: card(),
            patch: ContactPatchInput::default(),
        }),
        concat!(
            r#"{"people":{"update_contact":{"card":{"person":7,"account":"work","#,
            r#""card":"/dav/card/work/default/ada.vcf"},"patch":{}}}}"#
        ),
    );
}

#[test]
fn the_people_surface_crosses_as_its_word() {
    assert_eq!(
        serde_json::to_string(&Surface::People).unwrap(),
        r#""people""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""people""#).unwrap(),
        Surface::People
    );
}

#[test]
fn a_malformed_book_or_card_reference_is_refused_on_the_wire() {
    // Both halves validate on the way in, exactly as an event reference's do:
    // a blank account, a zero person, or an empty id cannot ride an intent
    // into the core and route a contact write by half a reference.
    let blank_account = concat!(
        r#"{"people":{"create_contact":{"book":{"account":"","#,
        r#""book":"/dav/card/work/default/"},"draft":{"organisation":"Astra"}}}}"#
    );
    assert!(serde_json::from_str::<Intent>(blank_account).is_err());
    let zero_person = concat!(
        r#"{"people":{"update_contact":{"card":{"person":0,"account":"work","#,
        r#""card":"/dav/card/work/default/ada.vcf"},"patch":{}}}}"#
    );
    assert!(serde_json::from_str::<Intent>(zero_person).is_err());
    let empty_card = concat!(
        r#"{"people":{"update_contact":{"card":{"person":7,"account":"work","#,
        r#""card":""},"patch":{}}}}"#
    );
    assert!(serde_json::from_str::<Intent>(empty_card).is_err());
    assert!(AddressBookRef::from_parts("work", String::new()).is_none());
    assert!(CardRef::from_parts(0, "work", "/dav/ada.vcf".to_owned()).is_none());
    assert!(CardRef::from_parts(7, "", "/dav/ada.vcf".to_owned()).is_none());
    assert!(CardRef::from_parts(7, "work", String::new()).is_none());
    // The well-formed halves read back as the host gave them.
    let book = book();
    assert_eq!(book.account.as_str(), "work");
    assert_eq!(book.book.as_str(), "/dav/card/work/default/");
    let card = card();
    assert_eq!(card.person.get(), 7);
    assert_eq!(card.account.as_str(), "work");
    assert_eq!(card.card.as_str(), "/dav/card/work/default/ada.vcf");
}
