// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings snapshot's account rows: the join of the app's
//! own account set with the host's account infos, the kinds and the legs
//! flags the row states, and the dav echo that never carries the credential.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{AccountKind, AccountLegs, DavSectionRow, GrantState};

use crate::calendar_fakes::CalendarFake;
use crate::protocol::SettingsIntent;
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{settings_fakes, Intent, Surface};

/// Builds an in-memory app over the given `(id, providers)` pairs, with the
/// scripted store installed and the surfaces recorded.
fn over(
    accounts: Vec<(&str, Vec<CalendarFake>)>,
    ops: FakeAccountOps,
) -> (crate::App<CalendarFake>, Arc<Mutex<Vec<Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let accounts = accounts
        .into_iter()
        .map(|(id, providers)| crate::settings_fakes::account(id, providers))
        .collect();
    let app = settings_app(accounts, Arc::new(ops), None, &surfaces);
    (app, surfaces)
}

#[tokio::test]
async fn the_rows_join_the_app_s_accounts_with_the_host_s_infos() {
    let ops = FakeAccountOps::new()
        .describing("plain")
        .with_kind(AccountKind::Jmap)
        .describing("jmap-one");
    let (app, _surfaces) = over(
        vec![
            ("plain", vec![mail_half()]),
            ("jmap-one", vec![mail_half()]),
            ("undescribed", vec![mail_half()]),
        ],
        ops,
    );

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();

    let rows = app.settings().accounts;
    assert_eq!(
        rows.iter()
            .map(|row| row.account.as_str())
            .collect::<Vec<_>>(),
        vec!["plain", "jmap-one", "undescribed"],
        "one row per app account, in the app's own order"
    );
    assert_eq!(
        rows[0].kind,
        AccountKind::ImapSmtp,
        "the store's default kind"
    );
    assert_eq!(rows[1].kind, AccountKind::Jmap, "the host's stated kind");
    assert_eq!(
        rows[2].kind,
        AccountKind::ImapSmtp,
        "an account the host did not describe renders the mail-only shape"
    );
    for row in &rows {
        assert_eq!(row.address, "info@test.local");
        assert!(row.dav.is_none());
        assert_eq!(row.dav_error, None);
    }
}

#[tokio::test]
async fn the_legs_flags_state_what_is_actually_standing() {
    let ops = FakeAccountOps::new()
        .describing("split")
        .describing("whole");
    let calendar_leg = CalendarFake::with_calendar_and_mail();
    let (app, _surfaces) = over(
        vec![("split", vec![mail_half()]), ("whole", vec![calendar_leg])],
        ops,
    );
    app.install_contacts(
        &engine_api::AccountId::try_from("split").unwrap(),
        Box::new(crate::people_fakes::PeopleFake::with_book()),
    );

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();

    let rows = app.settings().accounts;
    assert_eq!(
        rows[0].legs,
        AccountLegs {
            calendar: false,
            contacts: true
        },
        "the split account stands on its mail half and its contacts leg"
    );
    assert_eq!(
        rows[1].legs,
        AccountLegs {
            calendar: true,
            contacts: false
        },
        "the whole account's provider speaks the calendar domain"
    );
}

#[tokio::test]
async fn the_dav_row_echoes_the_stored_section_masked() {
    let ops = FakeAccountOps::new().describing("work");
    ops.storing(
        "work",
        Some("https://dav.example.test"),
        None,
        "ada",
        Some("app-password"),
    );
    let (app, _surfaces) = over(vec![("work", vec![mail_half()])], ops);

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();

    let row = &app.settings().accounts[0];
    assert_eq!(
        row.dav,
        Some(DavSectionRow {
            caldav_url: Some("https://dav.example.test".to_owned()),
            carddav_url: None,
            username: "ada".to_owned(),
            dav_password_set: true,
        })
    );
    // The masked echo is the row's own wire: no password field exists to
    // cross, and the surface pulls the snapshot rather than the store.
    let wire = serde_json::to_value(row).unwrap();
    assert!(wire
        .get("dav")
        .is_some_and(|dav| dav.get("password").is_none()));
}

#[tokio::test]
async fn an_info_for_an_account_the_app_does_not_hold_renders_no_row() {
    let ops = FakeAccountOps::new().describing("held").describing("gone");
    let (app, _surfaces) = over(vec![("held", vec![mail_half()])], ops);

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();

    let rows = app.settings().accounts;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account.as_str(), "held");
}

#[tokio::test]
async fn a_withheld_calendar_rides_the_running_account_s_row() {
    let ops = FakeAccountOps::new()
        .describing("work")
        .with_grant("work", GrantState::CalendarWithheld);
    let (app, _surfaces) = over(vec![("work", vec![mail_half()])], ops);

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();

    let rows = app.settings().accounts;
    assert_eq!(
        rows[0].grant,
        Some(GrantState::CalendarWithheld),
        "the row carries the standing refusal state the host answered"
    );
    // The healthy account beside it crosses no grant key: the absence is
    // the state.
    let ops = FakeAccountOps::new().describing("plain");
    let (app, _surfaces) = over(vec![("plain", vec![mail_half()])], ops);
    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
    let wire = serde_json::to_value(&app.settings().accounts[0]).unwrap();
    assert!(
        wire.get("grant").is_none(),
        "healthy crosses no key: {wire}"
    );
}

#[tokio::test]
async fn a_dead_grant_renders_the_expired_row_for_an_account_the_app_does_not_hold() {
    // The one exception to the join rule: an account whose grant the host
    // reports dead never dialled, so the app does not hold it, and the row
    // it still renders is the only surface that can offer the re-grant.
    let ops = FakeAccountOps::new()
        .describing("held")
        .describing_as("gone", "gone@example.test", AccountKind::Microsoft)
        .with_grant("gone", GrantState::SignInExpired);
    let (app, _surfaces) = over(vec![("held", vec![mail_half()])], ops);

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();

    let rows = app.settings().accounts;
    assert_eq!(rows.len(), 2, "the running account, then the expired row");
    assert_eq!(rows[0].account.as_str(), "held");
    let expired = &rows[1];
    assert_eq!(expired.account.as_str(), "gone");
    assert_eq!(expired.address, "gone@example.test", "the host's address");
    assert_eq!(expired.kind, AccountKind::Microsoft);
    assert_eq!(
        expired.legs,
        AccountLegs {
            calendar: false,
            contacts: false
        },
        "a dead grant dials no leg at all"
    );
    assert_eq!(expired.grant, Some(GrantState::SignInExpired));
}

#[tokio::test]
async fn an_added_account_s_row_publishes_without_a_settings_dispatch() {
    // The rows join the app's account set, so an add must republish the
    // settings surface beside the list: the accounts page that stands open
    // while a sign-in completes reads the new row without a second ask.
    let ops = FakeAccountOps::new();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let accounts = Vec::new();
    let app = settings_app(accounts, Arc::new(ops), None, &surfaces);

    app.add_new_account_deferred(settings_fakes::account("late", vec![mail_half()]))
        .await;

    assert!(
        surfaces.lock().unwrap().contains(&Surface::Settings),
        "the settings surface republishes with the account set"
    );
    assert!(
        app.settings()
            .accounts
            .iter()
            .any(|row| row.account.as_str() == "late"),
        "the new account's row stands without a settings dispatch"
    );
}

#[tokio::test]
async fn the_crypto_method_write_lands_on_the_row_and_an_unknown_word_is_refused() {
    let ops = FakeAccountOps::new().describing("work");
    let (app, _surfaces) = over(vec![("work", vec![mail_half()])], ops);

    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        app.settings().accounts[0].crypto_method,
        "none",
        "the default echo"
    );

    app.dispatch(Intent::Settings(SettingsIntent::SetCryptoMethod {
        account: engine_api::AccountId::try_from("work").unwrap(),
        method: "openpgp".to_owned(),
    }))
    .await
    .unwrap();
    let row = &app.settings().accounts[0];
    assert_eq!(row.crypto_method, "openpgp", "the write lands on the row");
    assert!(row.crypto_error.is_none());

    // An unknown method word is refused with the row error, and the standing
    // method does not move.
    app.dispatch(Intent::Settings(SettingsIntent::SetCryptoMethod {
        account: engine_api::AccountId::try_from("work").unwrap(),
        method: "carrier-pigeon".to_owned(),
    }))
    .await
    .unwrap();
    let row = &app.settings().accounts[0];
    assert_eq!(row.crypto_method, "openpgp", "the refusal changes nothing");
    assert!(
        row.crypto_error
            .as_deref()
            .is_some_and(|e| e.contains("none, openpgp, or smime")),
        "the refusal reads on the row: {:?}",
        row.crypto_error
    );

    // A good write after clears the error.
    app.dispatch(Intent::Settings(SettingsIntent::SetCryptoMethod {
        account: engine_api::AccountId::try_from("work").unwrap(),
        method: "none".to_owned(),
    }))
    .await
    .unwrap();
    let row = &app.settings().accounts[0];
    assert_eq!(row.crypto_method, "none");
    assert!(row.crypto_error.is_none());
}
