// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings half of the inbound protocol: the exact JSON wire
//! shape every [`SettingsIntent`](super::SettingsIntent) crosses the IPC in,
//! that each reads back identical, and that the list intent's own view-mode
//! wire beside the family stays untouched.

use super::{DavSectionInput, Intent, SettingsIntent, Surface, ThemeMode, ViewMode, WeekdayWire};
use crate::reference::CalendarRef;
use engine_api::AccountId;

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

/// A well-formed account id, the way every settings test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// The work account's calendar, the way the default-calendar literal names one.
fn calendar() -> CalendarRef {
    CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap()
}

#[test]
fn refresh_crosses_as_a_bare_word() {
    round_trips(
        &Intent::Settings(SettingsIntent::Refresh),
        r#"{"settings":"refresh"}"#,
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<Intent>(r#"{"settings":"Refresh"}"#).is_err());
}

#[test]
fn set_theme_carries_the_mode_as_a_plain_word() {
    round_trips(
        &Intent::Settings(SettingsIntent::SetTheme {
            mode: ThemeMode::FollowHost,
        }),
        r#"{"settings":{"set_theme":{"mode":"follow_host"}}}"#,
    );
    round_trips(
        &Intent::Settings(SettingsIntent::SetTheme {
            mode: ThemeMode::Light,
        }),
        r#"{"settings":{"set_theme":{"mode":"light"}}}"#,
    );
    round_trips(
        &Intent::Settings(SettingsIntent::SetTheme {
            mode: ThemeMode::Dark,
        }),
        r#"{"settings":{"set_theme":{"mode":"dark"}}}"#,
    );
    assert!(
        serde_json::from_str::<Intent>(r#"{"settings":{"set_theme":{"mode":"FollowHost"}}}"#)
            .is_err()
    );
}

#[test]
fn set_clock_and_set_quote_style_carry_their_flag() {
    round_trips(
        &Intent::Settings(SettingsIntent::SetClock { twenty_four: true }),
        r#"{"settings":{"set_clock":{"twenty_four":true}}}"#,
    );
    round_trips(
        &Intent::Settings(SettingsIntent::SetQuoteStyle { below: false }),
        r#"{"settings":{"set_quote_style":{"below":false}}}"#,
    );
}

#[test]
fn set_zone_carries_the_zone_verbatim() {
    // The type carries the string; the core validates the id on dispatch,
    // never here, so a host's typo reaches the core as-is to be refused typed
    // there rather than second-guessed on the wire.
    round_trips(
        &Intent::Settings(SettingsIntent::SetZone {
            zone: "Europe/Amsterdam".to_owned(),
        }),
        r#"{"settings":{"set_zone":{"zone":"Europe/Amsterdam"}}}"#,
    );
}

#[test]
fn set_first_day_of_week_carries_the_engine_s_weekday_word() {
    round_trips(
        &Intent::Settings(SettingsIntent::SetFirstDayOfWeek {
            weekday: WeekdayWire::Su,
        }),
        r#"{"settings":{"set_first_day_of_week":{"weekday":"su"}}}"#,
    );
    round_trips(
        &Intent::Settings(SettingsIntent::SetFirstDayOfWeek {
            weekday: WeekdayWire::Mo,
        }),
        r#"{"settings":{"set_first_day_of_week":{"weekday":"mo"}}}"#,
    );
    // The full word is not the wire, in either crate's spelling.
    assert!(serde_json::from_str::<Intent>(
        r#"{"settings":{"set_first_day_of_week":{"weekday":"Monday"}}}"#
    )
    .is_err());
}

#[test]
fn set_horizon_carries_the_day_count() {
    round_trips(
        &Intent::Settings(SettingsIntent::SetHorizon { days: 42 }),
        r#"{"settings":{"set_horizon":{"days":42}}}"#,
    );
}

#[test]
fn the_family_s_set_view_mode_keeps_the_list_arm_s_wire_untouched() {
    // The settings family wraps the mode in its object; the list intent's
    // bare-word wire beside it is unchanged, the additive rule the slice
    // ships under.
    round_trips(
        &Intent::Settings(SettingsIntent::SetViewMode {
            mode: ViewMode::Threaded,
        }),
        r#"{"settings":{"set_view_mode":{"mode":"threaded"}}}"#,
    );
    round_trips(
        &Intent::Settings(SettingsIntent::SetViewMode {
            mode: ViewMode::Flat,
        }),
        r#"{"settings":{"set_view_mode":{"mode":"flat"}}}"#,
    );
    round_trips(
        &Intent::SetViewMode(ViewMode::Threaded),
        r#"{"set_view_mode":"threaded"}"#,
    );
}

#[test]
fn set_default_calendar_names_the_calendar_or_clears_it() {
    round_trips(
        &Intent::Settings(SettingsIntent::SetDefaultCalendar {
            calendar: Some(calendar()),
        }),
        r#"{"settings":{"set_default_calendar":{"calendar":{"account":"work","key":"cal-1"}}}}"#,
    );
    // `None` unpins: the explicit null, not an absent key, so a cleared pin
    // and an unmentioned one never share a wire.
    round_trips(
        &Intent::Settings(SettingsIntent::SetDefaultCalendar { calendar: None }),
        r#"{"settings":{"set_default_calendar":{"calendar":null}}}"#,
    );
    // A blank account half is refused on the wire, exactly as every reference
    // is.
    let blank =
        r#"{"settings":{"set_default_calendar":{"calendar":{"account":"","key":"cal-1"}}}}"#;
    assert!(serde_json::from_str::<Intent>(blank).is_err());
}

#[test]
fn set_dav_carries_the_editor_s_section_or_clears_it() {
    // The full section: both origins and both credential fields.
    round_trips(
        &Intent::Settings(SettingsIntent::SetDav {
            account: account("work"),
            section: Some(DavSectionInput {
                caldav_url: Some("https://dav.example.test".to_owned()),
                carddav_url: Some("https://cards.example.test".to_owned()),
                username: Some("ada@example.test".to_owned()),
                password: Some("app-password".to_owned()),
            }),
        }),
        concat!(
            r#"{"settings":{"set_dav":{"account":"work","#,
            r#""section":{"caldav_url":"https://dav.example.test","#,
            r#""carddav_url":"https://cards.example.test","#,
            r#""username":"ada@example.test","password":"app-password"}}}}"#
        ),
    );
    // The minimal section: an edit of one field crosses no key for the rest,
    // exactly as a contact patch omits what it does not change.
    round_trips(
        &Intent::Settings(SettingsIntent::SetDav {
            account: account("work"),
            section: Some(DavSectionInput {
                username: Some("ada@example.test".to_owned()),
                ..DavSectionInput::default()
            }),
        }),
        r#"{"settings":{"set_dav":{"account":"work","section":{"username":"ada@example.test"}}}}"#,
    );
    // `None` clears the section: the legs' config goes, as the explicit null.
    round_trips(
        &Intent::Settings(SettingsIntent::SetDav {
            account: account("work"),
            section: None,
        }),
        r#"{"settings":{"set_dav":{"account":"work","section":null}}}"#,
    );
}

#[test]
fn remove_account_names_the_account() {
    round_trips(
        &Intent::Settings(SettingsIntent::RemoveAccount {
            account: account("work"),
        }),
        r#"{"settings":{"remove_account":{"account":"work"}}}"#,
    );
}

#[test]
fn a_malformed_account_id_is_refused_on_the_wire() {
    // The account half validates on the way in, the same rule every family's
    // references follow.
    let blank = r#"{"settings":{"remove_account":{"account":""}}}"#;
    assert!(serde_json::from_str::<Intent>(blank).is_err());
}

#[test]
fn the_settings_surface_crosses_as_its_word() {
    assert_eq!(
        serde_json::to_string(&Surface::Settings).unwrap(),
        r#""settings""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""settings""#).unwrap(),
        Surface::Settings
    );
}
