// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the snapshot rebuild: the section fold re-ordering the engine's
//! page into one alphabet plus one `#` bucket, the monogram rules (the real
//! characters, the address fallback, the empty signal), the account-count
//! disclosure, the empty display-name passthrough, and the pinned palette.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::people::MONOGRAM_PALETTE;
use crate::people_fakes::{account, bare_card, named_card, people_app, PeopleFake};
use crate::{App, Intent, PeopleIntent};

/// Installs `fake` as `account`'s contacts leg.
fn install(app: &App<PeopleFake>, account_id: &str, fake: PeopleFake) {
    app.install_contacts(&AccountId::try_from(account_id).unwrap(), Box::new(fake));
}

#[tokio::test]
async fn the_list_lays_out_by_the_fold_not_the_engine_page_order() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(named_card("c-1", "book-1", "Emil Plain", &["emil@ex.test"]))
        .holding(named_card("c-2", "book-1", "Emma B", &["emma@ex.test"]))
        .holding(named_card("c-3", "book-1", "Émile Zola", &["zola@ex.test"]))
        .holding(named_card(
            "c-4",
            "book-1",
            "Öberg Swede",
            &["oberg@ex.test"],
        ))
        .holding(named_card(
            "c-5",
            "book-1",
            "Oscar Plain",
            &["oscar@ex.test"],
        ))
        .holding(named_card(
            "c-6",
            "book-1",
            "Černý Czech",
            &["cerny@ex.test"],
        ))
        .holding(named_card(
            "c-7",
            "book-1",
            "Ñandú Bird",
            &["nandu@ex.test"],
        ))
        .holding(named_card(
            "c-8",
            "book-1",
            "Яна Иванова",
            &["yana@ex.test"],
        ))
        .holding(named_card(
            "c-9",
            "book-1",
            "3M Worker",
            &["worker3@ex.test"],
        ))
        .holding(named_card("c-10", "book-1", "", &["quiet@ex.test"]));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.people();
    let laid_out: Vec<(&str, &str)> = snapshot
        .rows
        .iter()
        .map(|row| (row.display_name.as_str(), row.section.as_str()))
        .collect();
    assert_eq!(
        laid_out,
        vec![
            // Č folds onto C, not D: the extended ranges sit on the base
            // letter they stand on.
            ("Černý Czech", "C"),
            ("Emil Plain", "E"),
            // Émile files between Emil and Emma: the fold re-orders the page,
            // whose raw code-point order placed him after everybody.
            ("Émile Zola", "E"),
            ("Emma B", "E"),
            // A real Ñ row keeps N: the leading base letter rule in both directions.
            ("Ñandú Bird", "N"),
            // The O-variants (Ò..Ö) fold onto O, not N, and lay out inside it.
            ("Öberg Swede", "O"),
            ("Oscar Plain", "O"),
            // No name: the display stays empty for the host's own words, and
            // the row files under the address's letter.
            ("", "Q"),
            // The one `#` bucket collects digits, symbols, and other scripts,
            // and lays out after the alphabet.
            ("3M Worker", "#"),
            ("Яна Иванова", "#"),
        ],
        "the rows lay out by the fold, in one alphabet with one # bucket"
    );
    let quiet = snapshot
        .rows
        .iter()
        .find(|row| row.primary_address.as_deref() == Some("quiet@ex.test"))
        .expect("the no-name person is on the list");
    assert_eq!(
        quiet.display_name, "",
        "the engine's address fallback never leaks into the name field"
    );
}

#[tokio::test]
async fn the_monogram_keeps_the_real_characters_and_follows_the_name_rules() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(named_card("c-1", "book-1", "Émile Zola", &["zola@ex.test"]))
        .holding(named_card("c-2", "book-1", "Madonna", &["madonna@ex.test"]))
        .holding(named_card("c-3", "book-1", "", &["quiet@ex.test"]))
        .holding(bare_card("c-4", "book-1"));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.people();
    let initials = |display: &str| {
        snapshot
            .rows
            .iter()
            .find(|row| row.display_name == display)
            .map(|row| row.monogram.initials.clone())
    };
    // First and last words, uppercased, real characters kept: the É survives
    // even though the section folded him under E.
    assert_eq!(initials("Émile Zola").as_deref(), Some("ÉZ"));
    assert_eq!(initials("Madonna").as_deref(), Some("M"));
    // No name: the address's first character; neither: the empty signal.
    assert_eq!(
        snapshot
            .rows
            .iter()
            .find(|row| row.primary_address.as_deref() == Some("quiet@ex.test"))
            .map(|row| row.monogram.initials.as_str()),
        Some("Q")
    );
    assert_eq!(
        snapshot
            .rows
            .iter()
            .find(|row| row.primary_address.is_none())
            .map(|row| row.monogram.initials.as_str()),
        Some("")
    );
}

#[tokio::test]
async fn every_monogram_colour_comes_from_the_pinned_palette_and_is_stable() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(named_card("c-1", "book-1", "Emil Plain", &["emil@ex.test"]))
        .holding(named_card("c-2", "book-1", "Emma B", &["emma@ex.test"]))
        .holding(bare_card("c-3", "book-1"));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let first: Vec<(u64, String)> = app
        .people()
        .rows
        .iter()
        .map(|row| {
            assert!(
                MONOGRAM_PALETTE.contains(&row.monogram.colour.as_str()),
                "the colour {} is not a palette slot",
                row.monogram.colour
            );
            (row.person.get(), row.monogram.colour.clone())
        })
        .collect();
    assert!(
        first.iter().any(|(_, colour)| !colour.is_empty()),
        "the fixture paints something"
    );

    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let second: Vec<(u64, String)> = app
        .people()
        .rows
        .iter()
        .map(|row| (row.person.get(), row.monogram.colour.clone()))
        .collect();
    assert_eq!(
        first, second,
        "a person's colour never moves between rebuilds"
    );
}

#[tokio::test]
async fn a_joined_person_discloses_two_accounts_and_a_single_one_discloses_none() {
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
        "Ada Elsewhere",
        &["ada@ex.test"],
    ));
    let app = people_app(
        vec![account("work", work.clone()), account("home", home.clone())],
        &surfaces,
    );
    install(&app, "work", work);
    install(&app, "home", home);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.people();
    // Distinct accounts across the person's sources, never source cards:
    // two cards over two accounts disclose 2, the same two cards over one
    // account would disclose 1, and one card discloses nothing.
    let joined: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|row| row.account_count.is_some())
        .collect();
    assert_eq!(joined.len(), 1, "only the joined person crosses a key");
    assert_eq!(joined[0].account_count, Some(2));
    assert!(
        snapshot
            .rows
            .iter()
            .all(|row| row.account_count.is_none() || row.account_count == Some(2)),
        "a single-account person carries no count"
    );
}
