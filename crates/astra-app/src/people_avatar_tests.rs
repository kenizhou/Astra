// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the avatar read: the monogram comes from the one derivation
//! the snapshot rows carry (a person paints the same initials and colour on
//! every surface), a stranger keys on the address alone, and a photo only
//! ever answers from the engine's cached read, never a fetch.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, ContactId};

use crate::people_fakes::{
    account, named_card, people_app, people_app_with_engine, with_photo, PeopleFake,
};
use crate::{App, Intent, PeopleIntent};

/// Installs `fake` as `account`'s contacts leg.
fn install(app: &App<PeopleFake>, account_id: &str, fake: PeopleFake) {
    app.install_contacts(&AccountId::try_from(account_id).unwrap(), Box::new(fake));
}

/// An app over one writable account holding Ada's card, synced and listed.
async fn ada_app() -> (App<PeopleFake>, PeopleFake) {
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
    (app, fake)
}

#[tokio::test]
async fn the_avatar_derives_the_monogram_the_row_carries() {
    let (app, _fake) = ada_app().await;

    let view = app.avatar_by_email("ada@ex.test").await;
    let snapshot = app.people();
    let row = snapshot
        .rows
        .iter()
        .find(|row| row.display_name == "Ada Lovelace")
        .expect("the synced person lists");
    assert_eq!(view.initials, row.monogram.initials);
    assert_eq!(view.colour, row.monogram.colour);
    assert_eq!(view.initials, "AL");
    assert_eq!(view.photo, None);
}

#[tokio::test]
async fn a_stranger_draws_the_address_initial_and_no_photo() {
    let (app, _fake) = ada_app().await;

    let view = app.avatar_by_email("stranger@ex.test").await;
    assert_eq!(view.initials, "S");
    assert_eq!(view.photo, None);
}

#[tokio::test]
async fn an_uncached_photo_leaves_the_monogram_standing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(with_photo(
        named_card("c-1", "book-1", "Ada Lovelace", &["ada@ex.test"]),
        "rev-1",
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    // The card advertises a photo and the provider serves one, but nothing
    // has filled the cache: the hot-path read does store work only, so the
    // monogram stands and no fetch happens.
    let view = app.avatar_by_email("ada@ex.test").await;
    assert_eq!(view.photo, None);
    assert_eq!(view.initials, "AL");
}

#[tokio::test]
async fn a_cached_photo_answers_the_read_with_its_file() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(with_photo(
        named_card("c-1", "book-1", "Ada Lovelace", &["ada@ex.test"]),
        "rev-1",
    ));
    let (app, engine) = people_app_with_engine(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake.clone());
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    // The background pass the engine's own doc describes: one fetch through
    // the provider fills the cache the avatar read then answers from.
    let account_id = AccountId::try_from("work").unwrap();
    let contact = ContactId::try_from("c-1").unwrap();
    let card = engine
        .contact_card(&account_id, &contact)
        .await
        .unwrap()
        .expect("the synced card");
    let media = card
        .media
        .values()
        .map(|property| &property.value)
        .find(|resource| resource.kind.as_deref() == Some("photo"))
        .expect("the card carries a photo resource");
    let fetched = engine
        .contact_photo(&fake, &account_id, &card, media)
        .await
        .unwrap()
        .expect("the fetch filled the cache");
    assert!(fetched.path.exists());

    let view = app.avatar_by_email("ada@ex.test").await;
    let photo = view.photo.expect("the cached file answers the read");
    assert!(std::path::Path::new(&photo).exists());
    assert_eq!(view.initials, "AL");
}
