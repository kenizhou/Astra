// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the composer finishing's settings arms: the app-level default
//! send account and each signature slot persist into the preferences
//! document before the surface answers, the republished snapshot carries
//! them (the pin in the block, the assignment on the account row), and the
//! last empty slot takes the account's signature row with it.

use std::sync::{Arc, Mutex};

use astra_composer::Signature;
use astra_viewmodel::AccountSignatures;
use engine_api::AccountId;

use crate::calendar_fakes::CalendarFake;
use crate::preferences::load_preferences;
use crate::protocol::SettingsIntent;
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{App, Intent, Surface};

/// One mail-half account named `work`, the shape every arm starts from.
fn app(prefs: Option<std::path::PathBuf>) -> App<CalendarFake> {
    let surfaces = recorded();
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    settings_app(
        vec![crate::settings_fakes::account("work", vec![mail_half()])],
        ops,
        prefs,
        &surfaces,
    )
}

/// The shared surface recorder, so the builder stays a one-liner.
fn recorded() -> Arc<Mutex<Vec<Surface>>> {
    Arc::new(Mutex::new(Vec::new()))
}

/// A scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-signatures-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Dispatches `intent` and asserts it landed.
async fn lands(app: &App<CalendarFake>, intent: SettingsIntent) {
    app.dispatch(Intent::Settings(intent)).await.unwrap();
}

/// The work signature the way every test writes one.
fn signature(plain: &str) -> Signature {
    Signature {
        body_html: format!("<p>{plain}</p>"),
        body_plain: plain.to_owned(),
    }
}

fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

#[tokio::test]
async fn the_default_send_account_persists_and_the_snapshot_carries_it() {
    let dir = scratch("send-account");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::SetSendAccount {
            account: Some(work()),
        },
    )
    .await;

    let snapshot = app.settings();
    assert_eq!(snapshot.settings.default_send_account, Some(work()));
    assert_eq!(snapshot.error, None, "the pin landed");
    assert_eq!(
        load_preferences(&path).default_send_account,
        Some(work()),
        "the pin is on disk before the surface answers"
    );

    // `None` unpins: the first standing account answers again.
    lands(&app, SettingsIntent::SetSendAccount { account: None }).await;
    assert_eq!(app.settings().settings.default_send_account, None);
    assert_eq!(load_preferences(&path).default_send_account, None);
}

#[tokio::test]
async fn each_signature_slot_persists_and_the_row_carries_the_assignment() {
    let dir = scratch("slots");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::SetSignature {
            account: work(),
            reply: false,
            signature: Some(signature("Ada")),
        },
    )
    .await;
    lands(
        &app,
        SettingsIntent::SetSignature {
            account: work(),
            reply: true,
            signature: Some(signature("Ada on replies")),
        },
    )
    .await;

    let snapshot = app.settings();
    let row = snapshot
        .accounts
        .iter()
        .find(|row| row.account == work())
        .expect("the work row stands");
    assert_eq!(
        row.signatures,
        Some(AccountSignatures {
            new_mail: Some(signature("Ada")),
            reply: Some(signature("Ada on replies")),
        }),
        "the row carries both slots"
    );
    assert_eq!(snapshot.error, None);

    // The assignment is on disk, the same one the snapshot shows.
    let persisted = load_preferences(&path);
    assert_eq!(
        persisted.seeded_signature(&work(), false),
        Some(signature("Ada"))
    );
    assert_eq!(
        persisted.seeded_signature(&work(), true),
        Some(signature("Ada on replies"))
    );
}

#[tokio::test]
async fn the_last_empty_slot_takes_the_row_with_it() {
    let dir = scratch("last-slot");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::SetSignature {
            account: work(),
            reply: false,
            signature: Some(signature("Ada")),
        },
    )
    .await;
    lands(
        &app,
        SettingsIntent::SetSignature {
            account: work(),
            reply: false,
            signature: None,
        },
    )
    .await;

    let snapshot = app.settings();
    let row = snapshot
        .accounts
        .iter()
        .find(|row| row.account == work())
        .expect("the work row still stands");
    assert_eq!(row.signatures, None, "no live assignment crosses no key");
    assert!(
        !load_preferences(&path).signatures.contains_key(&work()),
        "the file holds only live assignments"
    );
}
