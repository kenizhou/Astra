// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the blocked-sender preference: the address normalisation the
//! blocklist keys on, the BlockSender/UnblockSender arms persisting into the
//! document and answering on the settings block, and the empty-set
//! canonicalisation that keeps an untouched blocklist out of the file.

use std::sync::{Arc, Mutex};

use crate::preferences::{load_preferences, normalise_sender_address};
use crate::protocol::SettingsIntent;
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{App, Intent, Surface};

/// One mail-half account named `work`, the shape every blocking test starts
/// from, persisting to `prefs`.
fn app(prefs: Option<std::path::PathBuf>) -> App<crate::calendar_fakes::CalendarFake> {
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    settings_app(
        vec![crate::settings_fakes::account("work", vec![mail_half()])],
        ops,
        prefs,
        &surfaces,
    )
}

/// A scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-blocking-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Dispatches `intent` and asserts it landed.
async fn lands(app: &App<crate::calendar_fakes::CalendarFake>, intent: SettingsIntent) {
    app.dispatch(Intent::Settings(intent)).await.unwrap();
}

#[test]
fn the_address_normalisation_lifts_the_bare_addr_spec() {
    assert_eq!(
        normalise_sender_address("ada@example.test"),
        "ada@example.test"
    );
    assert_eq!(
        normalise_sender_address("  ADA@Example.test  "),
        "ada@example.test",
        "trimmed and lowercased"
    );
    assert_eq!(
        normalise_sender_address("Ada Lovelace <Ada@Example.test>"),
        "ada@example.test",
        "the addr-spec lifted out of the display form"
    );
    assert_eq!(
        normalise_sender_address("  <ada@example.test>  "),
        "ada@example.test"
    );
    assert_eq!(
        normalise_sender_address(""),
        "",
        "a blank address reads empty"
    );
    assert_eq!(normalise_sender_address("   "), "");
}

#[tokio::test]
async fn block_and_unblock_persist_and_answer_on_the_settings_block() {
    let dir = scratch("round-trip");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::BlockSender {
            address: "Ada Lovelace <ADA@Example.test>".to_owned(),
        },
    )
    .await;

    // The block answers on the settings block, normalised.
    let snapshot = app.settings();
    assert_eq!(
        snapshot.settings.blocked_senders,
        ["ada@example.test".to_owned()].into_iter().collect(),
        "the block rides the settings block, normalised"
    );
    assert_eq!(snapshot.error, None, "the write landed");

    // The block on screen is the block on disk.
    let persisted = load_preferences(&path);
    assert_eq!(
        persisted.blocked_senders,
        ["ada@example.test".to_owned()].into_iter().collect()
    );

    // A second sender joins; the set holds both.
    lands(
        &app,
        SettingsIntent::BlockSender {
            address: "bob@example.test".to_owned(),
        },
    )
    .await;
    assert_eq!(app.settings().settings.blocked_senders.len(), 2);

    // Unblock drops the one entry, leaving the other standing.
    lands(
        &app,
        SettingsIntent::UnblockSender {
            address: "ada@example.test".to_owned(),
        },
    )
    .await;
    let snapshot = app.settings();
    assert_eq!(
        snapshot.settings.blocked_senders,
        ["bob@example.test".to_owned()].into_iter().collect(),
        "the unblock drops only its own entry"
    );
    assert_eq!(
        load_preferences(&path).blocked_senders,
        ["bob@example.test".to_owned()].into_iter().collect()
    );
}

#[tokio::test]
async fn the_block_normalises_so_one_sender_is_one_entry() {
    let dir = scratch("normalise");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    // Three spellings of one sender land one entry.
    for address in [
        "ada@example.test",
        "ADA@Example.test",
        "Ada Lovelace <ada@example.test>",
    ] {
        lands(
            &app,
            SettingsIntent::BlockSender {
                address: address.to_owned(),
            },
        )
        .await;
    }
    assert_eq!(
        app.settings().settings.blocked_senders.len(),
        1,
        "every spelling of one sender agrees on one key"
    );

    // The unblock normalises too: a display form drops the bare entry.
    lands(
        &app,
        SettingsIntent::UnblockSender {
            address: "Ada <ADA@example.test>".to_owned(),
        },
    )
    .await;
    assert!(
        app.settings().settings.blocked_senders.is_empty(),
        "the display-form unblock drops the bare entry"
    );
}

#[tokio::test]
async fn a_blank_address_is_refused_and_stores_nothing() {
    let dir = scratch("blank");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::BlockSender {
            address: "   ".to_owned(),
        },
    )
    .await;
    assert!(
        app.settings().settings.blocked_senders.is_empty(),
        "a blank address stores no entry"
    );
    assert!(
        load_preferences(&path).blocked_senders.is_empty(),
        "a blank address persists no entry"
    );
}

#[tokio::test]
async fn an_empty_blocklist_leaves_the_file() {
    let dir = scratch("canonical");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::BlockSender {
            address: "ada@example.test".to_owned(),
        },
    )
    .await;
    lands(
        &app,
        SettingsIntent::UnblockSender {
            address: "ada@example.test".to_owned(),
        },
    )
    .await;

    // The last entry's removal takes the field out of the file, the same
    // canonicalisation the expansion maps follow.
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        !body.contains("blocked_senders"),
        "an empty blocklist writes no entry: {body}"
    );
}
