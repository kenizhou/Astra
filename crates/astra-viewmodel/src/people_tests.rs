// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the people view-model: the exact JSON wire shape a host pulls
//! after a `people` signal, that it reads back identical, the disclosure
//! facts the rows carry, and the default an app publishes before any
//! people state exists.

use engine_api::{AccountId, AddressBookId, ContactId, PersonId};

use crate::{
    CreateTargetRow, EditableCardRow, Monogram, PeopleSnapshot, PeopleWriteRow, PeopleWriteState,
    PersonRow, PersonSourceRow, PersonValueKind, PersonValueRow,
};

/// A well-formed account id, the way every people test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A well-formed address-book id, the way the create-target literal names one.
fn book(value: &str) -> AddressBookId {
    AddressBookId::try_from(value).unwrap()
}

/// A well-formed contact id, the way the source literals name one.
fn card(value: &str) -> ContactId {
    ContactId::try_from(value).unwrap()
}

#[test]
fn the_wire_shape_is_snake_case_and_carries_every_row_kind() {
    let work_card = "/dav/card/work/default/ada.vcf";
    let home_card = "/dav/card/home/default/ada.vcf";
    let snapshot = PeopleSnapshot {
        rows: vec![PersonRow {
            person: PersonId::new(7).unwrap(),
            display_name: "Ada Lovelace".to_owned(),
            monogram: Monogram {
                initials: "AL".to_owned(),
                colour: "teal".to_owned(),
            },
            account_count: Some(2),
            section: "A".to_owned(),
            primary_address: Some("ada@example.test".to_owned()),
        }],
        detail: Some(crate::PersonDetail {
            person: PersonId::new(7).unwrap(),
            display_name: "Ada Lovelace".to_owned(),
            sources: vec![
                PersonSourceRow {
                    account: account("work"),
                    card: card(work_card),
                    writable: true,
                },
                PersonSourceRow {
                    account: account("home"),
                    card: card(home_card),
                    writable: false,
                },
            ],
            values: vec![
                PersonValueRow {
                    kind: PersonValueKind::Email,
                    value: "ada@example.test".to_owned(),
                    sources: vec![
                        PersonSourceRow {
                            account: account("work"),
                            card: card(work_card),
                            writable: true,
                        },
                        PersonSourceRow {
                            account: account("home"),
                            card: card(home_card),
                            writable: false,
                        },
                    ],
                },
                PersonValueRow {
                    kind: PersonValueKind::Phone,
                    value: "+31 20 555 0100".to_owned(),
                    sources: vec![PersonSourceRow {
                        account: account("work"),
                        card: card(work_card),
                        writable: true,
                    }],
                },
            ],
            editable_cards: vec![EditableCardRow {
                person: PersonId::new(7).unwrap(),
                account: account("work"),
                card: card(work_card),
            }],
        }),
        write_states: vec![PeopleWriteRow {
            account: account("work"),
            state: PeopleWriteState::Writing,
        }],
        create_targets: vec![CreateTargetRow {
            account: account("work"),
            book: book("/dav/card/work/default/"),
            name: "Contacts".to_owned(),
            is_default: true,
        }],
    };

    let wire = serde_json::to_value(&snapshot).expect("the snapshot serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "rows": [
                {
                    "person": 7,
                    "display_name": "Ada Lovelace",
                    "monogram": { "initials": "AL", "colour": "teal" },
                    "account_count": 2,
                    "section": "A",
                    "primary_address": "ada@example.test"
                }
            ],
            "detail": {
                "person": 7,
                "display_name": "Ada Lovelace",
                "sources": [
                    {
                        "account": "work",
                        "card": "/dav/card/work/default/ada.vcf",
                        "writable": true
                    },
                    {
                        "account": "home",
                        "card": "/dav/card/home/default/ada.vcf",
                        "writable": false
                    }
                ],
                "values": [
                    {
                        "kind": "email",
                        "value": "ada@example.test",
                        "sources": [
                            {
                                "account": "work",
                                "card": "/dav/card/work/default/ada.vcf",
                                "writable": true
                            },
                            {
                                "account": "home",
                                "card": "/dav/card/home/default/ada.vcf",
                                "writable": false
                            }
                        ]
                    },
                    {
                        "kind": "phone",
                        "value": "+31 20 555 0100",
                        "sources": [
                            {
                                "account": "work",
                                "card": "/dav/card/work/default/ada.vcf",
                                "writable": true
                            }
                        ]
                    }
                ],
                "editable_cards": [
                    {
                        "person": 7,
                        "account": "work",
                        "card": "/dav/card/work/default/ada.vcf"
                    }
                ]
            },
            "write_states": [
                { "account": "work", "state": "writing" }
            ],
            "create_targets": [
                {
                    "account": "work",
                    "book": "/dav/card/work/default/",
                    "name": "Contacts",
                    "is_default": true
                }
            ]
        }),
    );

    let back: PeopleSnapshot = serde_json::from_value(wire).expect("the snapshot reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn the_write_state_words_cross_as_snake_case() {
    // Each write-state word is pinned both ways: a host branches on these, so
    // a renamed word must never slip out silently.
    for (state, word) in [
        (PeopleWriteState::Writing, r#""writing""#),
        (PeopleWriteState::FailedWithRetry, r#""failed_with_retry""#),
        (PeopleWriteState::Invalid, r#""invalid""#),
    ] {
        assert_eq!(serde_json::to_string(&state).unwrap(), word);
        assert_eq!(
            serde_json::from_str::<PeopleWriteState>(word).unwrap(),
            state
        );
    }
}

#[test]
fn the_value_kind_words_cross_as_snake_case() {
    // Each value-kind word is pinned both ways, the same discipline the write
    // states follow.
    for (kind, word) in [
        (PersonValueKind::Name, r#""name""#),
        (PersonValueKind::Email, r#""email""#),
        (PersonValueKind::Phone, r#""phone""#),
        (PersonValueKind::Organisation, r#""organisation""#),
        (PersonValueKind::Title, r#""title""#),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), word);
        assert_eq!(serde_json::from_str::<PersonValueKind>(word).unwrap(), kind);
    }
}

#[test]
fn a_row_of_one_account_crosses_no_account_count() {
    // The disclosure is for merges only: a person of one account crosses no
    // count key at all, and a person with no address crosses no address key,
    // exactly as an occurrence of a one-off event crosses no occurrence token.
    let row = PersonRow {
        person: PersonId::new(9).unwrap(),
        display_name: "Alan Turing".to_owned(),
        monogram: Monogram {
            initials: "AT".to_owned(),
            colour: "olive".to_owned(),
        },
        account_count: None,
        section: "A".to_owned(),
        primary_address: None,
    };
    let wire = serde_json::to_value(&row).expect("the row serialises");
    assert!(
        wire.get("account_count").is_none(),
        "one account crosses no count: {wire}"
    );
    assert!(
        wire.get("primary_address").is_none(),
        "no address crosses no key: {wire}"
    );
    let back: PersonRow = serde_json::from_value(wire).expect("the row reads back");
    assert_eq!(back, row);
}

#[test]
fn the_default_snapshot_crosses_as_an_empty_object() {
    // Before any people state exists the snapshot is empty: nothing crosses a
    // key except by being present, so a cold host renders an empty list, not a
    // zeroed one.
    let snapshot = PeopleSnapshot::default();
    assert!(snapshot.rows.is_empty());
    assert!(snapshot.detail.is_none());
    assert!(snapshot.write_states.is_empty());
    assert!(snapshot.create_targets.is_empty());

    let wire = serde_json::to_value(&snapshot).expect("the default serialises");
    assert_eq!(wire, serde_json::json!({}));

    let back: PeopleSnapshot =
        serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).expect("reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn a_nameless_row_carries_the_empty_signals() {
    // The engine invents no placeholder name and no initials: empty is the
    // signal, and the host supplies the words and its own person glyph. The
    // monogram's colour still crosses, because the avatar's fill does not
    // depend on there being letters to draw.
    let row = PersonRow {
        person: PersonId::new(11).unwrap(),
        display_name: String::new(),
        monogram: Monogram {
            initials: String::new(),
            colour: "slate".to_owned(),
        },
        account_count: None,
        section: "#".to_owned(),
        primary_address: Some("ops@example.test".to_owned()),
    };
    let wire = serde_json::to_value(&row).expect("the row serialises");
    assert_eq!(wire["display_name"], "");
    assert_eq!(wire["monogram"]["initials"], "");
    assert_eq!(wire["section"], "#");
    let back: PersonRow = serde_json::from_value(wire).expect("the row reads back");
    assert_eq!(back, row);
}

#[test]
fn a_value_without_provenance_and_a_detail_without_extras_skip_their_keys() {
    // A detail of one source with no values yet still opens: the empty lists
    // cross no keys, and a value the engine could not source crosses none
    // either.
    let detail = crate::PersonDetail {
        person: PersonId::new(13).unwrap(),
        display_name: "Only A Name".to_owned(),
        sources: vec![PersonSourceRow {
            account: account("work"),
            card: card("/dav/card/work/default/only.vcf"),
            writable: false,
        }],
        values: Vec::new(),
        editable_cards: Vec::new(),
    };
    let wire = serde_json::to_value(&detail).expect("the detail serialises");
    assert!(
        wire.get("values").is_none(),
        "no values cross no key: {wire}"
    );
    assert!(
        wire.get("editable_cards").is_none(),
        "no editable cards cross no key: {wire}"
    );
    let back: crate::PersonDetail = serde_json::from_value(wire).expect("the detail reads back");
    assert_eq!(back, detail);

    let value = PersonValueRow {
        kind: PersonValueKind::Organisation,
        value: "Astra".to_owned(),
        sources: Vec::new(),
    };
    let wire = serde_json::to_value(&value).expect("the value serialises");
    assert!(
        wire.get("sources").is_none(),
        "no provenance crosses no key: {wire}"
    );
    let back: PersonValueRow = serde_json::from_value(wire).expect("the value reads back");
    assert_eq!(back, value);
}
