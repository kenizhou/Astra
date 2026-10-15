// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings commands: the snapshot pull round trips the
//! core's published view whole, settings block, account rows, and about
//! block together, and the masked dav echo rides the row without a
//! password key.

use std::sync::Arc;

use astra_viewmodel::{
    AboutBlock, AccountKind, AccountLegs, AccountSettingsRow, DavSectionRow, SettingsBlock,
    SettingsSnapshot, ThemeMode, ViewMode, WeekdayWire,
};

use super::settings_of;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The fixture account the settings row names.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").expect("an account id")
}

#[test]
fn the_settings_pull_round_trips_the_published_snapshot() {
    let app = FakeApp::new();
    let snapshot = SettingsSnapshot {
        settings: SettingsBlock {
            theme: ThemeMode::Dark,
            twenty_four_clock: true,
            zone: "Europe/Amsterdam".to_owned(),
            first_day_of_week: WeekdayWire::Su,
            horizon_days: 31,
            quote_below: false,
            view_mode: ViewMode::Threaded,
            default_calendar: None,
        },
        accounts: vec![AccountSettingsRow {
            account: account(),
            address: "ada@example.test".to_owned(),
            kind: AccountKind::ImapSmtp,
            legs: AccountLegs {
                calendar: true,
                contacts: false,
            },
            dav: Some(DavSectionRow {
                caldav_url: Some("https://dav.example.test".to_owned()),
                carddav_url: None,
                username: "ada@example.test".to_owned(),
                dav_password_set: true,
            }),
            dav_error: None,
            refresh_error: None,
            grant: None,
        }],
        about: AboutBlock {
            version: "0.1.0".to_owned(),
            attributions: vec!["email-calendar-sync-engine".to_owned()],
        },
        error: None,
    };
    *app.settings.lock().unwrap() = snapshot.clone();
    let state = state("settings-pull", Arc::clone(&app), FakeStore::accepting());

    let pulled = settings_of(&state);
    assert_eq!(pulled, snapshot, "the pull answers whole");
    assert!(
        serde_json::to_value(&pulled.accounts[0])
            .expect("the row serialises")
            .get("dav")
            .is_some_and(|dav| dav.get("password").is_none()),
        "the masked row never carries a password field"
    );
}
