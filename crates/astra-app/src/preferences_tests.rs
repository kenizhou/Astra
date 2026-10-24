// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the persisted preferences: the defaults a first launch starts from, the
//! round trip through the TOML file, and the expansion map's inverted storage (the file
//! holds what is shut, so a folder nobody has touched shows what is inside it).

use std::path::PathBuf;

use astra_viewmodel::SyncBodyCap;

use crate::{
    load_preferences, preferences_path,
    reference::{CalendarRef, FolderRef},
    save_preferences, Preferences, ThemeMode, ViewMode, WeekdayWire,
};

/// A fresh scratch directory of its own, so one test's writes cannot reach another's.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-preferences-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_first_launch_starts_from_the_defaults() {
    let prefs = Preferences::default();
    assert_eq!(prefs.view_mode, ViewMode::default());
    assert!(
        prefs.account_expanded("acct-1"),
        "an untouched tree is open"
    );
    assert!(prefs.folder_expanded("acct-1", "archive"));
    assert!(prefs.selected_folder.is_none());
    assert_eq!(prefs.theme, ThemeMode::FollowHost);
    assert!(!prefs.twenty_four_clock);
    assert_eq!(prefs.zone, "Etc/UTC");
    assert_eq!(prefs.first_day_of_week, WeekdayWire::Mo);
    assert_eq!(prefs.horizon_days, 42);
    assert!(prefs.quote_below);
    assert!(prefs.default_calendar.is_none());
}

#[test]
fn a_missing_or_unreadable_file_yields_the_defaults() {
    let dir = scratch("missing");
    assert_eq!(
        load_preferences(dir.join("absent.toml")),
        Preferences::default()
    );

    let corrupt = dir.join("corrupt.toml");
    std::fs::write(&corrupt, "view_mode = [[not toml").unwrap();
    assert_eq!(load_preferences(&corrupt), Preferences::default());
}

#[test]
fn the_preferences_round_trip_through_the_file() {
    let dir = scratch("round-trip");
    let path = preferences_path(&dir);

    let mut prefs = Preferences {
        view_mode: ViewMode::Threaded,
        ..Preferences::default()
    };
    prefs.set_account_expanded("acct-1", false);
    prefs.set_folder_expanded("acct-1", "archive", false);
    prefs.set_selected_folder(Some(
        FolderRef::from_parts("acct-1", "clients".to_owned()).unwrap(),
    ));
    save_preferences(&path, &prefs).unwrap();

    assert!(path.ends_with("preferences.toml"));
    let back = load_preferences(&path);
    assert_eq!(back, prefs);
    assert_eq!(back.view_mode, ViewMode::Threaded);
    assert!(!back.account_expanded("acct-1"));
    assert!(!back.folder_expanded("acct-1", "archive"));
    let selected = back.selected_folder.expect("the folder persisted");
    assert_eq!(selected.account.as_str(), "acct-1");
    assert_eq!(selected.key, "clients");
}

#[test]
fn saving_creates_the_parent_directories_it_needs() {
    let dir = scratch("nested");
    let path = dir.join("deeper").join("still").join("preferences.toml");

    save_preferences(&path, &Preferences::default()).unwrap();

    assert_eq!(load_preferences(&path), Preferences::default());
}

#[test]
fn an_older_file_without_the_newer_fields_loads_with_defaults_for_them() {
    let dir = scratch("older-file");
    let path = preferences_path(&dir);
    // A document exactly as the build before the settings fields wrote it.
    std::fs::write(&path, "view_mode = \"threaded\"\n").unwrap();

    let prefs = load_preferences(&path);

    assert_eq!(prefs.view_mode, ViewMode::Threaded);
    assert!(prefs.collapsed_accounts.is_empty());
    assert!(prefs.collapsed_folders.is_empty());
    assert!(prefs.selected_folder.is_none());
    assert_eq!(prefs.theme, ThemeMode::FollowHost);
    assert!(!prefs.twenty_four_clock);
    assert_eq!(prefs.zone, "Etc/UTC");
    assert_eq!(prefs.first_day_of_week, WeekdayWire::Mo);
    assert_eq!(prefs.horizon_days, 42);
    assert!(prefs.quote_below);
    assert!(prefs.default_calendar.is_none());
}

#[test]
fn the_settings_fields_round_trip_through_the_file() {
    let dir = scratch("settings-round-trip");
    let path = preferences_path(&dir);

    let prefs = Preferences {
        theme: ThemeMode::Dark,
        twenty_four_clock: true,
        zone: "Europe/Amsterdam".to_owned(),
        first_day_of_week: WeekdayWire::Su,
        horizon_days: 31,
        quote_below: false,
        default_calendar: Some(CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap()),
        ..Preferences::default()
    };
    save_preferences(&path, &prefs).unwrap();

    let back = load_preferences(&path);
    assert_eq!(back, prefs);
    assert_eq!(back.theme, ThemeMode::Dark);
    assert!(back.twenty_four_clock);
    assert_eq!(back.zone, "Europe/Amsterdam");
    assert_eq!(back.first_day_of_week, WeekdayWire::Su);
    assert_eq!(back.horizon_days, 31);
    assert!(!back.quote_below);
    let default_calendar = back.default_calendar.expect("the pin persisted");
    assert_eq!(default_calendar.account.as_str(), "work");
    assert_eq!(default_calendar.key.as_str(), "cal-1");
}

#[test]
fn a_fresh_file_carries_every_default_but_no_default_calendar() {
    let dir = scratch("fresh-defaults");
    let path = preferences_path(&dir);

    save_preferences(&path, &Preferences::default()).unwrap();

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        !body.contains("default_calendar"),
        "an absent pin writes no entry: {body}"
    );
    assert!(body.contains("theme = \"follow_host\""));
    assert!(body.contains("twenty_four_clock = false"));
    assert!(body.contains("zone = \"Etc/UTC\""));
    assert!(body.contains("first_day_of_week = \"mo\""));
    assert!(body.contains("horizon_days = 42"));
    assert!(body.contains("quote_below = true"));
}

#[test]
fn view_mode_stays_a_key_of_its_own_on_disk() {
    // The grouping moves into the settings block on the wire alone; on disk it
    // stays the top-level key it always was, so the build that adds the
    // settings fields writes a file the build before it still reads.
    let dir = scratch("view-mode-home");
    let path = preferences_path(&dir);

    let prefs = Preferences {
        view_mode: ViewMode::Threaded,
        theme: ThemeMode::Dark,
        ..Preferences::default()
    };
    save_preferences(&path, &prefs).unwrap();

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("view_mode = \"threaded\""));
    assert_eq!(load_preferences(&path), prefs);
}

#[test]
fn expanding_again_leaves_nothing_behind_in_the_file() {
    let mut prefs = Preferences::default();
    prefs.set_account_expanded("acct-1", false);
    prefs.set_folder_expanded("acct-1", "archive", false);
    prefs.set_folder_expanded("acct-1", "clients", false);

    prefs.set_folder_expanded("acct-1", "archive", true);
    assert_eq!(prefs.collapsed_folders["acct-1"].len(), 1);

    // The account's last shut folder takes the account's own row with it, so opening
    // everything back up leaves the file exactly as it started.
    prefs.set_folder_expanded("acct-1", "clients", true);
    assert!(prefs.collapsed_folders.is_empty());
    prefs.set_account_expanded("acct-1", true);
    assert!(prefs.collapsed_accounts.is_empty());

    // And expanding something never shut writes nothing at all.
    prefs.set_folder_expanded("acct-3", "archive", true);
    assert!(prefs.collapsed_folders.is_empty());
}

#[test]
fn one_folder_shutting_leaves_its_siblings_and_other_accounts_open() {
    let mut prefs = Preferences::default();
    prefs.set_folder_expanded("acct-1", "archive", false);
    assert!(!prefs.folder_expanded("acct-1", "archive"));
    assert!(prefs.folder_expanded("acct-1", "clients"));
    assert!(prefs.folder_expanded("acct-2", "archive"));
}

#[test]
fn removing_an_account_forgets_its_trees_and_its_selected_folder() {
    let mut prefs = Preferences::default();
    prefs.set_account_expanded("acct-1", false);
    prefs.set_folder_expanded("acct-1", "archive", false);
    prefs.set_folder_expanded("acct-2", "archive", false);
    prefs.set_selected_folder(Some(
        FolderRef::from_parts("acct-1", "clients".to_owned()).unwrap(),
    ));

    prefs.remove_account("acct-1");

    // A re-add opens expanded rather than inheriting a shut tree nobody remembers.
    assert!(prefs.account_expanded("acct-1"));
    assert!(prefs.folder_expanded("acct-1", "archive"));
    assert!(prefs.selected_folder.is_none());
    // The account beside it keeps what it had.
    assert!(prefs.account_expanded("acct-2"));
    assert!(!prefs.folder_expanded("acct-2", "archive"));
}

#[test]
fn clearing_the_selected_folder_leaves_no_entry_behind() {
    let mut prefs = Preferences::default();
    prefs.set_selected_folder(Some(
        FolderRef::from_parts("acct-1", "clients".to_owned()).unwrap(),
    ));

    prefs.set_selected_folder(None);

    assert!(prefs.selected_folder.is_none());
}

#[test]
fn the_sync_settings_round_trip_through_the_file() {
    let dir = scratch("sync-round-trip");
    let path = preferences_path(&dir);
    let account = engine_api::AccountId::try_from("acct-1").unwrap();

    let mut prefs = Preferences::default();
    prefs.set_sync_settings(
        &account,
        Some(12),
        Some(SyncBodyCap::Bytes { octets: 1024 }),
        Some(90),
    );
    save_preferences(&path, &prefs).unwrap();

    let back = load_preferences(&path);
    assert_eq!(back, prefs);
    let row = back.sync.get(&account).expect("the row persisted");
    assert_eq!(row.depth_months, Some(12));
    assert_eq!(row.body_cap, Some(SyncBodyCap::Bytes { octets: 1024 }));
    assert_eq!(row.poll_minutes, Some(90));
}

#[test]
fn the_sync_cadence_clamps_on_write() {
    let account = engine_api::AccountId::try_from("acct-1").unwrap();
    let mut prefs = Preferences::default();

    prefs.set_sync_settings(&account, None, None, Some(5));
    assert_eq!(
        prefs.sync[&account].poll_minutes,
        Some(crate::preferences::POLL_MIN_MINUTES)
    );
    prefs.set_sync_settings(&account, None, None, Some(999));
    assert_eq!(
        prefs.sync[&account].poll_minutes,
        Some(crate::preferences::POLL_MAX_MINUTES)
    );
}

#[test]
fn an_all_default_sync_row_comes_out_of_the_file() {
    let account = engine_api::AccountId::try_from("acct-1").unwrap();
    let mut prefs = Preferences::default();
    prefs.set_sync_settings(&account, Some(12), None, None);
    assert!(prefs.sync.contains_key(&account));

    // A later edit that writes the depth back to its default is the unset:
    // the default and absent are one state, and the all-default row comes
    // out of the file, the same discipline the expansion maps follow.
    prefs.set_sync_settings(
        &account,
        Some(crate::accounts::NEW_ACCOUNT_SYNC_DEPTH_MONTHS),
        None,
        None,
    );
    assert!(prefs.sync.is_empty());
}

#[test]
fn removing_an_account_drops_its_sync_row() {
    let account = engine_api::AccountId::try_from("acct-1").unwrap();
    let mut prefs = Preferences::default();
    prefs.set_sync_settings(&account, Some(12), None, None);
    assert!(prefs.sync.contains_key(&account));

    assert!(prefs.remove_account("acct-1"));
    assert!(prefs.sync.is_empty(), "the account's row goes with it");
}

#[test]
fn the_platform_preferences_default_and_load_from_an_older_file() {
    let defaults = Preferences::default();
    assert!(!defaults.debug_enabled);
    assert!(!defaults.analytics_consented);
    assert_eq!(defaults.language, "en-GB");
    assert!(!defaults.zone_offered);
    assert!(!defaults.zone_picked);

    // A document exactly as the build before the platform fields wrote it.
    let dir = scratch("older-platform-file");
    let path = preferences_path(&dir);
    std::fs::write(&path, "view_mode = \"threaded\"\n").unwrap();
    let prefs = load_preferences(&path);
    assert_eq!(prefs.view_mode, ViewMode::Threaded);
    assert!(!prefs.debug_enabled);
    assert!(!prefs.analytics_consented);
    assert_eq!(prefs.language, "en-GB");
    assert!(!prefs.zone_offered);
    assert!(!prefs.zone_picked);
}

#[test]
fn the_platform_preferences_round_trip_through_the_file() {
    let dir = scratch("platform-round-trip");
    let path = preferences_path(&dir);
    let prefs = Preferences {
        debug_enabled: true,
        analytics_consented: true,
        language: "fr-FR".to_owned(),
        zone_offered: true,
        zone_picked: true,
        ..Preferences::default()
    };
    save_preferences(&path, &prefs).unwrap();

    let back = load_preferences(&path);
    assert_eq!(back, prefs);
    assert!(back.debug_enabled);
    assert!(back.analytics_consented);
    assert_eq!(back.language, "fr-FR");
    assert!(back.zone_offered);
    assert!(back.zone_picked);
}

#[test]
fn a_default_platform_preference_leaves_the_file() {
    let dir = scratch("platform-defaults-leave");
    let path = preferences_path(&dir);

    // A fresh file carries none of the platform keys: absent is each
    // default, the same discipline the expansion maps follow.
    save_preferences(&path, &Preferences::default()).unwrap();
    let fresh = std::fs::read_to_string(&path).unwrap();
    for key in [
        "debug_enabled",
        "analytics_consented",
        "language",
        "zone_offered",
        "zone_picked",
    ] {
        assert!(!fresh.contains(key), "the default writes no entry: {fresh}");
    }

    // A value written back to its default is the unset: the file loses the
    // key again, so the default and an explicit equal are one state.
    let mut prefs = Preferences {
        debug_enabled: true,
        analytics_consented: true,
        language: "fr-FR".to_owned(),
        zone_offered: true,
        zone_picked: true,
        ..Preferences::default()
    };
    save_preferences(&path, &prefs).unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("debug_enabled = true"));
    assert!(written.contains("analytics_consented = true"));
    assert!(written.contains("language = \"fr-FR\""));
    assert!(written.contains("zone_offered = true"));
    assert!(written.contains("zone_picked = true"));

    prefs.debug_enabled = false;
    prefs.analytics_consented = false;
    prefs.language = "en-GB".to_owned();
    prefs.zone_offered = false;
    prefs.zone_picked = false;
    save_preferences(&path, &prefs).unwrap();
    let back = std::fs::read_to_string(&path).unwrap();
    assert_eq!(load_preferences(&path), Preferences::default());
    for key in [
        "debug_enabled",
        "analytics_consented",
        "language",
        "zone_offered",
        "zone_picked",
    ] {
        assert!(!back.contains(key), "the default leaves the file: {back}");
    }
}
