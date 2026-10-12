// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings snapshot's account rows: the join of the app's
//! own account set with the host's account infos, the kinds and the legs
//! flags the row states, and the dav echo that never carries the credential.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{AccountKind, AccountLegs, DavSectionRow};

use crate::calendar_fakes::CalendarFake;
use crate::protocol::SettingsIntent;
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{Intent, Surface};

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
