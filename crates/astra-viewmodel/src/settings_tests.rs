// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings view-model: the exact JSON wire shape a host pulls
//! after a `settings` signal, that it reads back identical, the theme and
//! weekday words pinned, and the dav echo that never carries a password back.

use engine_api::{AccountId, CalendarId};

use astra_composer::Signature;

use crate::{
    AboutBlock, AccountKind, AccountLegs, AccountSettingsRow, AccountSignatures, DavSectionRow,
    DefaultCalendar, GrantState, SettingsBlock, SettingsError, SettingsSnapshot, ThemeMode,
    ViewMode, WeekdayWire,
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
                key: calendar("cal-1"),
            }),
            default_send_account: Some(account("work")),
            plugins_enabled: true,
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
            dav_error: Some("the re-dial could not reach the origin".to_owned()),
            refresh_error: Some(
                "the diary could not be re-expanded under the new zone; set the zone again to retry"
                    .to_owned(),
            ),
            grant: None,
            crypto_method: "none".to_owned(),
            crypto_error: None,
            signatures: Some(AccountSignatures {
                new_mail: Some(Signature {
                    body_html: "<p>Ada</p>".to_owned(),
                    body_plain: "Ada".to_owned(),
                }),
                reply: None,
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
        error: Some(SettingsError::PersistFailed),
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
                "plugins_enabled": true,
                "default_calendar": { "account": "work", "key": "cal-1" },
                "default_send_account": "work"
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
                    },
                    "dav_error": "the re-dial could not reach the origin",
                    "refresh_error": "the diary could not be re-expanded under the new zone; set the zone again to retry",
                    "crypto_method": "none",
                    "signatures": { "new_mail": { "body_html": "<p>Ada</p>", "body_plain": "Ada" } }
                }
            ],
            "about": {
                "version": "0.1.0",
                "attributions": ["email-calendar-sync-engine", "pi", "web"]
            },
            "error": "persist_failed"
        }),
    );

    let back: SettingsSnapshot = serde_json::from_value(wire).expect("the snapshot reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn the_error_words_cross_both_ways_and_absent_states_cross_no_key() {
    for (error, word) in [
        (SettingsError::PersistFailed, r#""persist_failed""#),
        (SettingsError::ZoneUnresolvable, r#""zone_unresolvable""#),
        (
            SettingsError::HorizonOutOfBounds,
            r#""horizon_out_of_bounds""#,
        ),
        (SettingsError::RemoveFailed, r#""remove_failed""#),
    ] {
        assert_eq!(serde_json::to_string(&error).unwrap(), word);
        assert_eq!(serde_json::from_str::<SettingsError>(word).unwrap(), error);
    }
    let clean = SettingsSnapshot::default();
    let wire = serde_json::to_value(&clean).expect("the clean snapshot serialises");
    assert!(wire.get("error").is_none());
    assert!(wire.get("accounts").is_none());
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
        (AccountKind::Microsoft, r#""microsoft""#),
        (AccountKind::Google, r#""google""#),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), word);
        assert_eq!(serde_json::from_str::<AccountKind>(word).unwrap(), kind);
    }
}

#[test]
fn the_grant_states_cross_both_ways_and_absent_means_healthy() {
    // The refusal states a row can carry, pinned both ways: the surfaces
    // branch on these words, so a renamed one must never slip out
    // silently.
    for (state, word) in [
        (GrantState::CalendarWithheld, r#""calendar_withheld""#),
        (GrantState::SignInExpired, r#""sign_in_expired""#),
    ] {
        assert_eq!(serde_json::to_string(&state).unwrap(), word);
        assert_eq!(serde_json::from_str::<GrantState>(word).unwrap(), state);
    }
    // A healthy account crosses no grant key at all: the absence is the
    // state, and a snapshot from a build before the field parses as
    // healthy rather than refusing.
    let mut healthy = AccountSettingsRow {
        account: account("work"),
        address: "ada@example.test".to_owned(),
        kind: AccountKind::Microsoft,
        legs: AccountLegs {
            calendar: false,
            contacts: false,
        },
        dav: None,
        dav_error: None,
        refresh_error: None,
        grant: None,
        signatures: None,
        crypto_method: "none".to_owned(),
        crypto_error: None,
    };
    let wire = serde_json::to_value(&healthy).expect("the healthy row serialises");
    assert!(
        wire.get("grant").is_none(),
        "no refusal crosses no key: {wire}"
    );
    assert_eq!(
        serde_json::from_str::<AccountSettingsRow>(
            r#"{"account":"work","address":"a@e.test","kind":"microsoft",
                "legs":{"calendar":false,"contacts":false}}"#
        )
        .expect("a pre-grant row parses")
        .grant,
        None,
        "an old wire reads back healthy"
    );
    healthy.grant = Some(GrantState::CalendarWithheld);
    let wire = serde_json::to_value(&healthy).expect("the withheld row serialises");
    assert_eq!(wire["grant"], "calendar_withheld");
    assert_eq!(
        serde_json::from_value::<AccountSettingsRow>(wire).expect("the row reads back"),
        healthy
    );
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
        dav_error: None,
        refresh_error: None,
        grant: None,
        signatures: None,
        crypto_method: "none".to_owned(),
        crypto_error: None,
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
        default_send_account: None,
        plugins_enabled: true,
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

#[test]
fn the_shared_web_fixture_parses_as_the_snapshot() {
    // The web service's settings.test.ts parses this same file, so the
    // fixture the surface mirrors and the snapshot the host publishes can
    // never drift apart.
    let raw = include_str!("../../../client/web/src/services/__fixtures__/settings_snapshot.json");
    let back: SettingsSnapshot = serde_json::from_str(raw).expect("the fixture parses");
    let wire = serde_json::to_value(&back).expect("the snapshot serialises");
    let raw: serde_json::Value = serde_json::from_str(raw).expect("the fixture is JSON");
    assert_eq!(wire, raw, "the fixture round trips value for value");
}
