// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings view-model: the exact JSON wire shape a host pulls
//! after a `settings` signal, that it reads back identical, the theme and
//! weekday words pinned, and the dav echo that never carries a password back.

use engine_api::{AccountId, CalendarId};

use crate::{
    AboutBlock, AccountKind, AccountLegs, AccountSettingsRow, DavSectionRow, DefaultCalendar,
    SettingsBlock, SettingsSnapshot, ThemeMode, ViewMode, WeekdayWire,
};

/// A well-formed account id, the way every settings test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A well-formed calendar id, the way the default-calendar literal names one.
fn calendar(value: &str) -> CalendarId {
    CalendarId::try_from(value).unwrap()
}

#[test]
fn the_snapshot_crosses_its_full_shape_and_reads_back() {
    let snapshot = SettingsSnapshot {
        settings: SettingsBlock {
            theme: ThemeMode::Dark,
            twenty_four_clock: true,
            zone: "Europe/Amsterdam".to_owned(),
            first_day_of_week: WeekdayWire::Su,
            horizon_days: 31,
            quote_below: false,
            view_mode: ViewMode::Threaded,
            default_calendar: Some(DefaultCalendar {
                account: account("work"),
                calendar: calendar("cal-1"),
            }),
        },
        accounts: vec![AccountSettingsRow {
            account: account("work"),
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
        }],
        about: AboutBlock {
            version: "0.1.0".to_owned(),
            attributions: vec![
                "email-calendar-sync-engine".to_owned(),
                "pi".to_owned(),
                "web".to_owned(),
            ],
        },
    };

    let wire = serde_json::to_value(&snapshot).expect("the snapshot serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "settings": {
                "theme": "dark",
                "twenty_four_clock": true,
                "zone": "Europe/Amsterdam",
                "first_day_of_week": "su",
                "horizon_days": 31,
                "quote_below": false,
                "view_mode": "threaded",
                "default_calendar": { "account": "work", "calendar": "cal-1" }
            },
            "accounts": [
                {
                    "account": "work",
                    "address": "ada@example.test",
                    "kind": "imap_smtp",
                    "legs": { "calendar": true, "contacts": false },
                    "dav": {
                        "caldav_url": "https://dav.example.test",
                        "username": "ada@example.test",
                        "dav_password_set": true
                    }
                }
            ],
            "about": {
                "version": "0.1.0",
                "attributions": ["email-calendar-sync-engine", "pi", "web"]
            }
        }),
    );

    let back: SettingsSnapshot = serde_json::from_value(wire).expect("the snapshot reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn the_theme_words_cross_both_ways_and_default_to_the_host() {
    // Each word is pinned both ways: a host branches on these, so a renamed
    // word must never slip out silently.
    for (mode, word) in [
        (ThemeMode::FollowHost, r#""follow_host""#),
        (ThemeMode::Light, r#""light""#),
        (ThemeMode::Dark, r#""dark""#),
    ] {
        assert_eq!(serde_json::to_string(&mode).unwrap(), word);
        assert_eq!(serde_json::from_str::<ThemeMode>(word).unwrap(), mode);
    }
    assert_eq!(ThemeMode::default(), ThemeMode::FollowHost);
}

#[test]
fn the_weekday_words_mirror_the_engine_s_serde() {
    // The mirror is pinned against the engine's own type, so the words can
    // never drift: a weekday stated to the settings surface and one stated to
    // the calendar's repeat rules spell the same day the same way.
    for (day, engine_day) in [
        (WeekdayWire::Mo, engine_api::Weekday::Mo),
        (WeekdayWire::Tu, engine_api::Weekday::Tu),
        (WeekdayWire::We, engine_api::Weekday::We),
        (WeekdayWire::Th, engine_api::Weekday::Th),
        (WeekdayWire::Fr, engine_api::Weekday::Fr),
        (WeekdayWire::Sa, engine_api::Weekday::Sa),
        (WeekdayWire::Su, engine_api::Weekday::Su),
    ] {
        let word = serde_json::to_string(&day).unwrap();
        assert_eq!(word, serde_json::to_string(&engine_day).unwrap());
        assert_eq!(serde_json::from_str::<WeekdayWire>(&word).unwrap(), day);
    }
    assert_eq!(WeekdayWire::default(), WeekdayWire::Mo);
    assert!(serde_json::from_str::<WeekdayWire>(r#""Monday""#).is_err());
}

#[test]
fn the_account_kinds_cross_both_ways() {
    // The same discipline the theme words follow.
    for (kind, word) in [
        (AccountKind::ImapSmtp, r#""imap_smtp""#),
        (AccountKind::Jmap, r#""jmap""#),
        (AccountKind::Dav, r#""dav""#),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), word);
        assert_eq!(serde_json::from_str::<AccountKind>(word).unwrap(), kind);
    }
}

#[test]
fn a_row_s_dav_echo_never_carries_the_password_back() {
    // The row answers whether a password is stored; the credential itself has
    // no field to cross, whatever the stored section holds.
    let row = AccountSettingsRow {
        account: account("work"),
        address: "ada@example.test".to_owned(),
        kind: AccountKind::Jmap,
        legs: AccountLegs {
            calendar: true,
            contacts: true,
        },
        dav: Some(DavSectionRow {
            caldav_url: Some("https://dav.example.test".to_owned()),
            carddav_url: Some("https://dav.example.test".to_owned()),
            username: "ada@example.test".to_owned(),
            dav_password_set: true,
        }),
    };
    let wire = serde_json::to_value(&row).expect("the row serialises");
    assert!(
        wire.get("dav")
            .is_some_and(|dav| dav.get("password").is_none()),
        "the echo crosses no password: {wire}"
    );
    assert_eq!(wire["dav"]["dav_password_set"], true);
    let back: AccountSettingsRow = serde_json::from_value(wire).expect("the row reads back");
    assert_eq!(back, row);

    // An account with no dav section crosses no dav key at all.
    let plain = AccountSettingsRow { dav: None, ..row };
    let wire = serde_json::to_value(&plain).expect("the plain row serialises");
    assert!(
        wire.get("dav").is_none(),
        "no section crosses no key: {wire}"
    );
    let back: AccountSettingsRow = serde_json::from_value(wire).expect("the row reads back");
    assert_eq!(back, plain);
}

#[test]
fn an_unpinned_default_calendar_and_empty_attributions_cross_no_key() {
    // The optional keys cross only when they hold something, exactly as the
    // people rows' disclosure keys do.
    let block = SettingsBlock {
        theme: ThemeMode::Light,
        twenty_four_clock: false,
        zone: "Etc/UTC".to_owned(),
        first_day_of_week: WeekdayWire::Mo,
        horizon_days: 42,
        quote_below: true,
        view_mode: ViewMode::Flat,
        default_calendar: None,
    };
    let wire = serde_json::to_value(&block).expect("the block serialises");
    assert!(
        wire.get("default_calendar").is_none(),
        "no pin crosses no key: {wire}"
    );
    let back: SettingsBlock = serde_json::from_value(wire).expect("the block reads back");
    assert_eq!(back, block);

    let about = AboutBlock {
        version: "0.1.0".to_owned(),
        attributions: Vec::new(),
    };
    let wire = serde_json::to_value(&about).expect("the about block serialises");
    assert!(
        wire.get("attributions").is_none(),
        "no names cross no key: {wire}"
    );
    assert_eq!(wire["version"], "0.1.0");
    let back: AboutBlock = serde_json::from_value(wire).expect("the about block reads back");
    assert_eq!(back, about);
}
