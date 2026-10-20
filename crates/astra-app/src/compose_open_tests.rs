// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the mailto open: the app-boundary prefill shape the core parse
//! fills, the dropped-extra-headers behaviour the parser owns shown through
//! the boundary, and the From the default send account resolves.

use std::sync::{Arc, Mutex};

use crate::{dispatch_fakes as fakes, Preferences};

#[tokio::test]
async fn a_mailto_link_opens_from_the_first_account_with_the_parser_s_five_fields() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![
            fakes::account("work", fakes::FakeProvider::with(vec![fakes::inbox()])),
            fakes::account("home", fakes::FakeProvider::with(vec![fakes::inbox()])),
        ],
        &surfaces,
    );

    let open = app
        .open_mailto("mailto:ada@example.test?cc=tim@example.test&subject=Lunch&body=Noon%3F")
        .await
        .expect("the link opens a composition");

    assert_eq!(
        open.from.as_str(),
        "work",
        "unset, the first account answers"
    );
    assert_eq!(open.to, "ada@example.test");
    assert_eq!(open.cc, "tim@example.test");
    assert_eq!(open.bcc, "");
    assert_eq!(open.subject, "Lunch");
    assert_eq!(open.body, "Noon?");
}

#[tokio::test]
async fn headers_the_parser_does_not_honour_never_reach_the_open() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with(vec![fakes::inbox()]),
        )],
        &surfaces,
    );

    let open = app
        .open_mailto(
            "mailto:ada@example.test?from=spoofer@example.test&in-reply-to=fake@x.test&x-anything=1&subject=Real",
        )
        .await
        .expect("the link still opens");

    // The shape carries the account and the five fields alone: a link's
    // `from` cannot name the sender (the resolved default stands), and no
    // threading or private header has a field to land in.
    assert_eq!(open.from.as_str(), "work");
    assert_eq!(open.to, "ada@example.test");
    assert_eq!(open.cc, "");
    assert_eq!(open.bcc, "");
    assert_eq!(open.subject, "Real");
    assert_eq!(open.body, "");
}

#[tokio::test]
async fn the_default_send_account_preference_names_the_account_a_mailto_opens_from() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let dir = fakes::scratch("mailto-default");
    let path = dir.join("preferences.toml");
    let mut prefs = Preferences::default();
    prefs.set_send_account(Some(engine_api::AccountId::try_from("home").unwrap()));
    crate::save_preferences(&path, &prefs).unwrap();

    let app = fakes::app_with_prefs(
        vec![
            fakes::account("work", fakes::FakeProvider::with(vec![fakes::inbox()])),
            fakes::account("home", fakes::FakeProvider::with(vec![fakes::inbox()])),
        ],
        Some(path),
        &surfaces,
    );

    let open = app
        .open_mailto("mailto:ada@example.test")
        .await
        .expect("the link opens from the preference's account");
    assert_eq!(open.from.as_str(), "home");
    assert_eq!(open.to, "ada@example.test");
}

#[tokio::test]
async fn a_uri_that_is_not_a_mailto_link_opens_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with(vec![fakes::inbox()]),
        )],
        &surfaces,
    );

    assert!(app
        .open_mailto("https://astra.example/compose")
        .await
        .is_none());
}
