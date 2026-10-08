// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the persisted preferences: the defaults a first launch starts from, the
//! round trip through the TOML file, and the expansion map's inverted storage (the file
//! holds what is shut, so a folder nobody has touched shows what is inside it).

use std::path::PathBuf;

use crate::{
    load_preferences, preferences_path, reference::FolderRef, save_preferences, Preferences,
    ViewMode,
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
    std::fs::write(&path, "view_mode = \"threaded\"\n").unwrap();

    let prefs = load_preferences(&path);

    assert_eq!(prefs.view_mode, ViewMode::Threaded);
    assert!(prefs.collapsed_accounts.is_empty());
    assert!(prefs.collapsed_folders.is_empty());
    assert!(prefs.selected_folder.is_none());
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
