// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the suggestion passthrough: the read takes the completion
//! token (never the field), forwards it verbatim, caps at the asked limit,
//! and returns the engine's own ranking untouched.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::people_fakes::{account, named_card, people_app, PeopleFake};
use crate::{App, Intent, PeopleIntent};

/// Installs `fake` as `account`'s contacts leg.
fn install(app: &App<PeopleFake>, account_id: &str, fake: PeopleFake) {
    app.install_contacts(&AccountId::try_from(account_id).unwrap(), Box::new(fake));
}

#[tokio::test]
async fn suggestions_come_back_in_the_engines_ranking_untouched() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(named_card("c-1", "book-1", "Émile Zola", &["zola@ex.test"]))
        .holding(named_card("c-2", "book-1", "Zoe Plain", &["zoe2@ex.test"]))
        .holding(named_card("c-3", "book-1", "Ann Other", &["azola@ex.test"]));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let addresses: Vec<String> = app
        .recipient_suggestions("zo", 10)
        .await
        .iter()
        .map(|suggestion| suggestion.email.as_str().to_owned())
        .collect();
    // The engine ranks the two token prefixes over the substring, settling
    // the tie by address ascending; an alphabetical re-sort of the whole list
    // would put the substring match first.
    assert_eq!(
        addresses,
        vec![
            "zoe2@ex.test".to_owned(),
            "zola@ex.test".to_owned(),
            "azola@ex.test".to_owned(),
        ],
        "the ranking is the engine's, not ours"
    );
}

#[tokio::test]
async fn the_limit_caps_the_list_the_engine_ranks() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book()
        .holding(named_card("c-1", "book-1", "Émile Zola", &["zola@ex.test"]))
        .holding(named_card("c-2", "book-1", "Zoe Plain", &["zoe2@ex.test"]))
        .holding(named_card("c-3", "book-1", "Ann Other", &["azola@ex.test"]));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let two = app.recipient_suggestions("zo", 2).await;
    assert_eq!(two.len(), 2);
    assert_eq!(
        two[0].email.as_str(),
        "zoe2@ex.test",
        "the cap keeps the head of the ranking"
    );
}

#[tokio::test]
async fn the_read_takes_the_token_verbatim_never_the_field() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = PeopleFake::with_book().holding(named_card(
        "c-1",
        "book-1",
        "Émile Zola",
        &["zola@ex.test"],
    ));
    let app = people_app(vec![account("work", fake.clone())], &surfaces);
    install(&app, "work", fake);
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    // The comma shape is a whole recipient field's text, not a token: the
    // read forwards it verbatim, and it matches nothing. A splitter here
    // would have found Émile.
    assert!(app.recipient_suggestions("Zola, zo", 10).await.is_empty());
    // A blank is not a search for everyone.
    assert!(app.recipient_suggestions("", 10).await.is_empty());
    assert!(app.recipient_suggestions("   ", 10).await.is_empty());
    // The token itself matches.
    assert_eq!(app.recipient_suggestions("zola", 10).await.len(), 1);
}
