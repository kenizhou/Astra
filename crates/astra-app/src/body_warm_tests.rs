// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the warm and cap pass, against the engine's in-memory store:
//! bodies present post-warm, the cap enforced in both directions, the off
//! state metadata-only, and the warm driven by the missing list so a pass
//! over an already-warm store asks the provider for nothing.

use std::sync::{Arc, Mutex};

use astra_viewmodel::SyncBodyCap;

use crate::dispatch_fakes as fakes;
use crate::protocol::SettingsIntent;
use engine_api::AccountId;

use crate::{App, Intent};

/// One message `key` about `subject`, `octets` big, with a raw source of
/// the same size so the store's size column and the blob agree.
fn sized(key: &str, subject: &str, octets: usize) -> engine_api::Message {
    let mut message = fakes::mail(key, "inbox", subject);
    message.size = Some(octets as u64);
    message
}

/// An app over one work account holding `messages`, each with a raw source
/// of `octets(key)` bytes.
fn warmed_app(
    messages: Vec<engine_api::Message>,
) -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<crate::Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let sources = messages
        .iter()
        .map(|message| {
            let octets = usize::try_from(message.size.unwrap_or(64)).unwrap();
            (
                message.id.key().as_str().to_owned(),
                format!(
                    "Subject: {}\r\n\r\n{}",
                    message.envelope.subject.clone().unwrap_or_default(),
                    "x".repeat(octets)
                )
                .into_bytes(),
            )
        })
        .collect();
    let built = fakes::app(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources),
        )],
        &surfaces,
    );
    (built, surfaces)
}

/// The work account's id.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// Sets the account's body cap and waits for the settings surface to answer.
async fn set_cap(app: &App<fakes::FakeProvider>, cap: SyncBodyCap) {
    app.dispatch(Intent::Settings(SettingsIntent::SetSyncSettings {
        account: work(),
        depth_months: None,
        body_cap: Some(cap),
        poll_minutes: None,
    }))
    .await
    .unwrap();
}

/// The account's mail rows still missing a cached body or source.
async fn missing(app: &App<fakes::FakeProvider>) -> Vec<String> {
    app.engine
        .mail_missing_body(std::slice::from_ref(&work()), 10)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.mail.key.as_str().to_owned())
        .collect()
}

/// How many source fetches the account's provider has served.
async fn fetches(app: &App<fakes::FakeProvider>) -> usize {
    app.account_handle(&work())
        .await
        .expect("the work account stands")
        .providers[0]
        .fetches
        .load(std::sync::atomic::Ordering::SeqCst)
}

#[tokio::test]
async fn a_pass_warms_the_missing_bodies() {
    let (app, _surfaces) = warmed_app(vec![sized("m1", "monitor the launch", 64)]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        missing(&app).await,
        Vec::<String>::new(),
        "the pass leaves no row without its body and source"
    );
}

#[tokio::test]
async fn the_cap_skips_what_grew_past_it_and_takes_it_back() {
    let (app, _surfaces) = warmed_app(vec![
        sized("small", "the small one", 32),
        sized("big", "the big one", 96),
    ]);
    set_cap(&app, SyncBodyCap::Bytes { octets: 64 }).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        missing(&app).await,
        ["big".to_owned()],
        "the small one warms; the big one stays metadata"
    );

    set_cap(&app, SyncBodyCap::Everything).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        missing(&app).await,
        Vec::<String>::new(),
        "raising the cap to everything fetches the big one back"
    );

    set_cap(&app, SyncBodyCap::Bytes { octets: 64 }).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        missing(&app).await,
        ["big".to_owned()],
        "lowering the cap again drops the big one's source"
    );
}

#[tokio::test]
async fn an_off_cap_is_metadata_only() {
    let (app, _surfaces) = warmed_app(vec![
        sized("m1", "monitor the launch", 32),
        sized("m2", "lunch on thursday", 32),
    ]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(missing(&app).await, Vec::<String>::new());

    set_cap(&app, SyncBodyCap::Off).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let mut still_missing = missing(&app).await;
    still_missing.sort();
    assert_eq!(
        still_missing,
        ["m1".to_owned(), "m2".to_owned()],
        "an off account warms nothing and drops what an open cached"
    );
}

#[tokio::test]
async fn the_warm_asks_for_nothing_the_store_already_holds() {
    let (app, _surfaces) = warmed_app(vec![sized("m1", "monitor the launch", 32)]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let after_first = fetches(&app).await;
    assert!(
        after_first >= 1,
        "the first pass fetched the missing source"
    );

    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        fetches(&app).await,
        after_first,
        "a pass over a warm store warms nothing: the work list is empty"
    );
}
