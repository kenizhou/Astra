// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the write paths: the patch's base re-read from the store (the
//! named card, never the merged person), only-changed fields and the
//! nothing-changed rule, the surname rule, the findable-again refusal typed
//! `Invalid`, and the write-state mapping including the write that landed
//! unconfirmed.

use std::sync::{Arc, Mutex};

use astra_viewmodel::PeopleWriteState;
use engine_api::{AccountId, ContactField, FieldPatch};
use engine_core::contact::{ContactName, NameComponentKind};

use crate::people_fakes::{
    account, named_card, people_app, structured_card, with_organisation, PeopleFake,
};
use crate::protocol::{ContactDraftInput, ContactNameInput, ContactPatchInput};
use crate::reference::{AddressBookRef, CardRef};
use crate::{App, Intent, PeopleIntent, Surface};

/// The work account's id, the way the fixtures name it.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// Installs `fake` as `account`'s contacts leg.
fn install(app: &App<PeopleFake>, account_id: &str, fake: PeopleFake) {
    app.install_contacts(&AccountId::try_from(account_id).unwrap(), Box::new(fake));
}

/// The person id the joined Ada row carries, after a refresh.
fn joined_ada(app: &App<PeopleFake>) -> u64 {
    app.people()
        .rows
        .iter()
        .find(|row| row.account_count.is_some())
        .map(|row| row.person.get())
        .expect("the joined person is on the list")
}

#[tokio::test]
async fn the_patches_base_is_the_named_card_not_the_merged_person() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let work = PeopleFake::with_book().holding(with_organisation(
        named_card("c-1", "book-1", "Ada Lovelace", &["ada@ex.test"]),
        "Ministry of Compute",
        "Chief",
    ));
    let home = PeopleFake::with_book().holding(with_organisation(
        named_card("h-1", "book-1", "Ada Elsewhere", &["ada@ex.test"]),
        "Home Org",
        "Painter",
    ));
    let app = people_app(
        vec![account("work", work.clone()), account("home", home.clone())],
        &surfaces,
    );
    install(&app, "work", work.clone());
    install(&app, "home", home);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let person = joined_ada(&app);

    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: CardRef::from_parts(person, "work", "c-1".to_owned()).unwrap(),
        patch: ContactPatchInput {
            emails: Some(vec!["ada2@ex.test".to_owned()]),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();

    let patches = work.patches.lock().unwrap();
    assert_eq!(patches.len(), 1);
    let (base, _) = &patches[0];
    assert_eq!(base.id.as_str(), "c-1", "the patch names the card");
    // The merged person carries both organisations; the base re-read from
    // the store carries only the named card's own, and exactly its one
    // address: the write would file one account's details in another's book.
    let organisation = base
        .organizations
        .values()
        .next()
        .map(|property| property.value.name.clone())
        .expect("the base carries the card's organisation");
    assert_eq!(organisation, "Ministry of Compute");
    assert_eq!(base.emails.len(), 1);
}

#[tokio::test]
async fn a_patch_names_only_changed_fields_and_nothing_changed_sends_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(named_card(
        "c-1",
        "book-1",
        "Ada Lovelace",
        &["ada@ex.test"],
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake.clone());
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let person = app.people().rows[0].person.get();
    let card = || CardRef::from_parts(person, "work", "c-1".to_owned()).unwrap();

    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: card(),
        patch: ContactPatchInput {
            emails: Some(vec!["ada@ex.test".to_owned(), "ada2@ex.test".to_owned()]),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();
    {
        let patches = fake.patches.lock().unwrap();
        assert_eq!(patches.len(), 1);
        let (_, patch) = &patches[0];
        assert_eq!(
            patch.fields.len(),
            1,
            "only the changed field is named: {patch:?}"
        );
        assert!(patch.fields.contains_key(&ContactField::Emails));
    }

    // A patch whose one list equals what the card already carries names
    // nothing, and nothing is sent: no engine call, no write row.
    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: card(),
        patch: ContactPatchInput {
            emails: Some(vec!["ada@ex.test".to_owned(), "ada2@ex.test".to_owned()]),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();
    assert_eq!(
        fake.patches.lock().unwrap().len(),
        1,
        "an edit that changed nothing sent nothing"
    );
    assert!(app.people().write_states.is_empty());
}

#[tokio::test]
async fn a_structured_name_is_written_only_with_a_surname() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(named_card(
            "c-1",
            "book-1",
            "Ada Lovelace",
            &["ada@ex.test"],
        ))
        .holding(structured_card(
            "c-2",
            "book-1",
            "Dr",
            "Ada",
            "Lovelace",
            &["structured@ex.test"],
        ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake.clone());
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let rows = app.people().rows;
    let ada = rows
        .iter()
        .find(|row| row.primary_address.as_deref() == Some("ada@ex.test"))
        .map(|row| row.person.get())
        .expect("the formatted card stands as its own person");
    let structured = rows
        .iter()
        .find(|row| row.primary_address.as_deref() == Some("structured@ex.test"))
        .map(|row| row.person.get())
        .expect("the structured card stands as its own person");

    // The formatted-only card, edited with no surname, keeps its formatted
    // name: nothing changed, so nothing is sent at all.
    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: CardRef::from_parts(ada, "work", "c-1".to_owned()).unwrap(),
        patch: ContactPatchInput {
            name: Some(ContactNameInput {
                given: "Ada Lovelace".to_owned(),
                surname: None,
            }),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();
    assert_eq!(
        fake.patches.lock().unwrap().len(),
        0,
        "an unchanged formatted name emits no components"
    );

    // With a surname, the structured form is written, rebuilt on top of the
    // card's own components: the prefix the form cannot state survives.
    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: CardRef::from_parts(structured, "work", "c-2".to_owned()).unwrap(),
        patch: ContactPatchInput {
            name: Some(ContactNameInput {
                given: "Adae".to_owned(),
                surname: Some("Lovelace".to_owned()),
            }),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();
    let name = recorded_name(&fake, 0);
    let kinds: Vec<(NameComponentKind, String)> = name
        .components
        .iter()
        .map(|component| (component.kind.clone(), component.value.clone()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (NameComponentKind::Prefix, "Dr".to_owned()),
            (NameComponentKind::Given, "Adae".to_owned()),
            (NameComponentKind::Surname, "Lovelace".to_owned()),
        ]
    );
    assert_eq!(name.full.as_deref(), Some("Dr Adae Lovelace"));

    // Without a surname, the structured card is written back formatted-only:
    // the components the form cannot hold are not re-emitted.
    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: CardRef::from_parts(structured, "work", "c-2".to_owned()).unwrap(),
        patch: ContactPatchInput {
            name: Some(ContactNameInput {
                given: "Ada Lovelace".to_owned(),
                surname: None,
            }),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();
    let name = recorded_name(&fake, 1);
    assert!(
        name.components.is_empty(),
        "a name without a surname is written formatted alone"
    );
    assert_eq!(name.full.as_deref(), Some("Ada Lovelace"));
}

/// The name value the `at`th recorded patch set, decoded as the engine's own
/// name shape.
fn recorded_name(fake: &PeopleFake, at: usize) -> ContactName {
    let patches = fake.patches.lock().unwrap();
    match &patches[at].1.fields[&ContactField::Name] {
        FieldPatch::Set(value) => serde_json::from_value(value.clone()).unwrap(),
        FieldPatch::Clear => panic!("the name field was cleared, not set"),
    }
}

#[tokio::test]
async fn an_unfindable_create_is_refused_as_invalid_and_a_findable_one_lands() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book();
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake.clone());
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let book = AddressBookRef::from_parts("work", "book-1".to_owned()).unwrap();

    app.dispatch(Intent::People(PeopleIntent::CreateContact {
        book: book.clone(),
        draft: ContactDraftInput::default(),
    }))
    .await
    .unwrap();
    assert_eq!(
        app.people().write_states,
        vec![astra_viewmodel::PeopleWriteRow {
            account: work(),
            state: PeopleWriteState::Invalid,
        }],
        "the core refused the form before anything was sent"
    );
    assert!(
        fake.creates.lock().unwrap().is_empty(),
        "the refusal crossed no wire"
    );

    // A name alone is findable; the create lands and the row settles clean.
    app.dispatch(Intent::People(PeopleIntent::CreateContact {
        book,
        draft: ContactDraftInput {
            name: Some(ContactNameInput {
                given: "New Person".to_owned(),
                surname: None,
            }),
            emails: vec!["fresh@ex.test".to_owned()],
            ..ContactDraftInput::default()
        },
    }))
    .await
    .unwrap();
    let creates = fake.creates.lock().unwrap();
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].address_book.as_str(), "book-1");
    let card = &creates[0].card;
    // No surname: the name travels formatted alone, never components the
    // form never held.
    let name = card.name.as_ref().expect("the name was written");
    assert!(name.components.is_empty());
    assert_eq!(name.full.as_deref(), Some("New Person"));
    assert!(
        card.emails
            .values()
            .any(|property| property.value.address == "fresh@ex.test"),
        "the create carries the address"
    );
    drop(creates);
    assert!(
        app.people().write_states.is_empty(),
        "a cleanly settled write crosses no row"
    );
    assert!(
        app.people()
            .rows
            .iter()
            .any(|row| row.display_name == "New Person"),
        "the created person is findable again"
    );
}

#[tokio::test]
async fn a_refused_write_fails_and_a_landed_unconfirmed_one_waits_for_a_reread() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().failing_writes(1);
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake.clone());
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let book = AddressBookRef::from_parts("work", "book-1".to_owned()).unwrap();
    let draft = ContactDraftInput {
        name: Some(ContactNameInput {
            given: "Refused".to_owned(),
            surname: None,
        }),
        ..ContactDraftInput::default()
    };

    // The engine refused the write outright: the change went nowhere, and
    // the row says so with the re-read as its recovery.
    app.dispatch(Intent::People(PeopleIntent::CreateContact {
        book: book.clone(),
        draft: draft.clone(),
    }))
    .await
    .unwrap();
    assert_eq!(
        app.people().write_states[0].state,
        PeopleWriteState::FailedWithRetry
    );

    // The write lands but the local refetch refuses: landed but unconfirmed,
    // the same row, and the refresh's re-read resolves it.
    let fake = fake.failing_fetches(1);
    install(&app, "work", fake.clone());
    app.dispatch(Intent::People(PeopleIntent::CreateContact { book, draft }))
        .await
        .unwrap();
    assert_eq!(
        app.people().write_states[0].state,
        PeopleWriteState::FailedWithRetry,
        "a landed write missing its local confirmation is unconfirmed, not clean"
    );
    assert_eq!(
        app.people().write_states[0].account,
        work(),
        "the status sits beside the list on the account's row"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::People),
        "each transition published the surface"
    );

    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert!(
        app.people().write_states.is_empty(),
        "the re-read resolved the unconfirmed row"
    );
    assert!(
        app.people()
            .rows
            .iter()
            .any(|row| row.display_name == "Refused"),
        "the landed write is there to keep"
    );
}
