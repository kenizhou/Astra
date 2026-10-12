// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the editor's seed: the stored card a `CardRef` names read back
//! through the engine and rendered as the editor's own input subset, so the
//! form opens on what the card carries rather than the merged detail.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::people_fakes::{
    account, named_card, people_app, structured_card, with_organisation, PeopleFake,
};
use crate::{App, CardRef, Intent, PeopleIntent};

/// Installs `fake` as `account`'s contacts leg.
fn install(app: &App<PeopleFake>, account_id: &str, fake: PeopleFake) {
    app.install_contacts(&AccountId::try_from(account_id).unwrap(), Box::new(fake));
}

/// The card reference of `card` on the work account, for the person the
/// store joined it into.
fn card_ref(app: &App<PeopleFake>, card: &str) -> CardRef {
    CardRef {
        person: app.people().rows[0].person,
        account: AccountId::try_from("work").unwrap(),
        card: engine_api::ContactId::try_from(card).unwrap(),
    }
}

#[tokio::test]
async fn a_structured_card_seeds_the_editor_s_fields() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(with_organisation(
        structured_card(
            "c-1",
            "book-1",
            "Dr",
            "Ada",
            "Lovelace",
            &["ada@ex.test", "ada.l@ex.test"],
        ),
        "Ministry of Compute",
        "Chief Mathematician",
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let seed = app
        .card_seed(&card_ref(&app, "c-1"))
        .await
        .expect("the stored card seeds the editor");
    // The structured name seeds its two fields; the prefix the form cannot
    // state stays behind, exactly as it does for a patch.
    assert_eq!(seed.name.as_ref().expect("the name seeds").given, "Ada");
    assert_eq!(
        seed.name
            .as_ref()
            .expect("the name seeds")
            .surname
            .as_deref(),
        Some("Lovelace")
    );
    assert_eq!(seed.organisation.as_deref(), Some("Ministry of Compute"));
    assert_eq!(seed.role.as_deref(), Some("Chief Mathematician"));
    assert_eq!(
        seed.emails,
        vec!["ada@ex.test".to_owned(), "ada.l@ex.test".to_owned()],
        "the addresses seed in card order"
    );
    assert!(seed.phones.is_empty());
}

#[tokio::test]
async fn a_formatted_only_name_seeds_whole_into_the_given_field() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(named_card(
        "c-1",
        "book-1",
        "Ada Lovelace",
        &["ada@ex.test"],
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let seed = app
        .card_seed(&card_ref(&app, "c-1"))
        .await
        .expect("the stored card seeds the editor");
    // The seeding rule: a card with no structured name holds its whole name
    // in the given field, and crosses no surname key at all.
    assert_eq!(
        seed.name.as_ref().expect("the name seeds").given,
        "Ada Lovelace"
    );
    assert_eq!(seed.name.as_ref().expect("the name seeds").surname, None);
    assert_eq!(seed.organisation, None);
    assert_eq!(seed.role, None);
    assert_eq!(seed.emails, vec!["ada@ex.test".to_owned()]);
}

#[tokio::test]
async fn a_card_nobody_stores_seeds_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(named_card(
        "c-1",
        "book-1",
        "Ada Lovelace",
        &["ada@ex.test"],
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let absent = CardRef {
        person: app.people().rows[0].person,
        account: AccountId::try_from("work").unwrap(),
        card: engine_api::ContactId::try_from("/gone.vcf").unwrap(),
    };
    assert!(
        app.card_seed(&absent).await.is_none(),
        "a card the store no longer holds seeds nothing"
    );
}
