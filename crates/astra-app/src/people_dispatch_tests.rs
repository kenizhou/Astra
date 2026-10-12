// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the dispatch arms: every arm publishes the people surface, the
//! search hands its query to the engine rather than filtering rows, an open
//! resolves a person id a merge retired, and a close clears the detail.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::people_fakes::{account, named_card, people_app, with_organisation, PeopleFake};
use crate::{App, Intent, PeopleIntent, Surface};

/// Installs `fake` as `account`'s contacts leg.
fn install(app: &App<PeopleFake>, account_id: &str, fake: PeopleFake) {
    app.install_contacts(&AccountId::try_from(account_id).unwrap(), Box::new(fake));
}

#[tokio::test]
async fn a_refresh_publishes_the_people_surface_with_the_synced_rows() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(named_card(
        "c-1",
        "book-1",
        "Ada Lovelace",
        &["ada@ex.test"],
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    assert!(
        !surfaces.lock().unwrap().contains(&Surface::People),
        "nothing published before the dispatch"
    );

    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    assert!(surfaces.lock().unwrap().contains(&Surface::People));
    assert_eq!(app.people().rows.len(), 1);
    assert_eq!(app.people().rows[0].display_name, "Ada Lovelace");
}

#[tokio::test]
async fn a_search_hands_the_query_to_the_engine_and_never_refilters() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(with_organisation(
            named_card("c-1", "book-1", "Ada Lovelace", &["ada@ex.test"]),
            "Ministry of Compute",
            "Chief",
        ))
        .holding(named_card(
            "c-2",
            "book-1",
            "Grace Hopper",
            &["grace@ex.test"],
        ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(app.people().rows.len(), 2);

    // The needle matches only the card's organisation, which no row's
    // display name carries: a host filtering its own rows over the display
    // name would have dropped her, and only the engine's match finds her.
    app.dispatch(Intent::People(PeopleIntent::Search {
        query: "ministry".to_owned(),
    }))
    .await
    .unwrap();

    let snapshot = app.people();
    assert_eq!(snapshot.rows.len(), 1);
    assert_eq!(snapshot.rows[0].display_name, "Ada Lovelace");
    assert!(surfaces.lock().unwrap().contains(&Surface::People));

    // An empty query is the unfiltered list again.
    app.dispatch(Intent::People(PeopleIntent::Search {
        query: String::new(),
    }))
    .await
    .unwrap();
    assert_eq!(app.people().rows.len(), 2);
}

#[tokio::test]
async fn an_open_resolves_a_person_id_a_merge_retired() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let work = PeopleFake::with_book().holding(named_card(
        "w-1",
        "book-1",
        "Ada Lovelace",
        &["ada@ex.test"],
    ));
    let home = PeopleFake::with_book().holding(named_card(
        "h-1",
        "book-1",
        "Grace Hopper",
        &["grace@ex.test"],
    ));
    let app = people_app(
        vec![account("work", work.clone()), account("home", home.clone())],
        &surfaces,
    );
    install(&app, "work", work);
    install(&app, "home", home.clone());
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let retired = app
        .people()
        .rows
        .iter()
        .find(|row| row.display_name == "Ada Lovelace")
        .map(|row| row.person)
        .expect("Ada stood alone before the merge");

    // Grace's card takes the address Ada already holds: the two join, the
    // component keeps the smaller of the two old ids (hers, from the account
    // that sorts first), and the id the host was holding retires behind it.
    home.serving(named_card(
        "h-1",
        "book-1",
        "Grace Hopper",
        &["ada@ex.test"],
    ));
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::People(PeopleIntent::OpenPerson { person: retired }))
        .await
        .unwrap();
    let snapshot = app.people();
    let detail = snapshot.detail.as_ref().expect("the retired id resolved");
    assert_ne!(detail.person, retired, "the detail names the survivor");
    assert_eq!(
        detail.sources.len(),
        2,
        "the survivor spans both accounts' cards"
    );
    assert!(surfaces.lock().unwrap().contains(&Surface::People));
}

#[tokio::test]
async fn a_close_clears_the_detail() {
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
    let person = app.people().rows[0].person;
    app.dispatch(Intent::People(PeopleIntent::OpenPerson { person }))
        .await
        .unwrap();
    assert!(app.people().detail.is_some());

    app.dispatch(Intent::People(PeopleIntent::ClosePerson))
        .await
        .unwrap();

    let snapshot = app.people();
    assert!(snapshot.detail.is_none());
    assert_eq!(snapshot.rows.len(), 1, "the list stays");
}
