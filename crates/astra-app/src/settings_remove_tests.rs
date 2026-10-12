// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the remove-account verb: the stored config goes through the
//! seam, the app's account entry and both legs go, the preferences'
//! account-scoped entries go with the selection that pointed there, the
//! engine's domains for the account go, and a failed removal answers typed
//! with nothing gone.

use std::sync::{Arc, Mutex};

use astra_viewmodel::SettingsError;

use crate::calendar_fakes::CalendarFake;
use crate::preferences::load_preferences;
use crate::protocol::{DavSectionInput, SettingsIntent};
use crate::reference::{FolderRef, MessageRef};
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{App, Intent, Surface};
use engine_api::AccountId;

/// A fresh section naming the CalDAV origin, so the removed account has
/// legs to tear down.
fn section() -> DavSectionInput {
    DavSectionInput {
        caldav_url: Some("https://dav.example.test".to_owned()),
        carddav_url: None,
        username: Some("ada".to_owned()),
        password: Some("app-password".to_owned()),
    }
}

/// Two accounts, `work` with a dav section to install and `personal` plain,
/// over the preferences file in `dir`.
async fn two_accounts(
    dir: &std::path::Path,
) -> (
    App<CalendarFake>,
    Arc<Mutex<Vec<Surface>>>,
    Arc<FakeAccountOps>,
) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let ops = Arc::new(
        FakeAccountOps::new()
            .describing("work")
            .describing("personal"),
    );
    let app = settings_app(
        vec![
            crate::settings_fakes::account("work", vec![mail_half()]),
            crate::settings_fakes::account("personal", vec![mail_half()]),
        ],
        Arc::clone(&ops),
        Some(dir.join("preferences.toml")),
        &surfaces,
    );
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: AccountId::try_from("work").unwrap(),
        section: Some(section()),
    }))
    .await
    .unwrap();
    (app, surfaces, ops)
}

/// A scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-remove-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Dispatches the removal of `work` and asserts it landed.
async fn remove_work(app: &App<CalendarFake>) {
    app.dispatch(Intent::Settings(SettingsIntent::RemoveAccount {
        account: AccountId::try_from("work").unwrap(),
    }))
    .await
    .unwrap();
}

#[tokio::test]
async fn remove_account_tears_down_every_named_half() {
    let dir = scratch("full");
    let (app, _surfaces, ops) = two_accounts(&dir).await;

    // The account holds account-scoped preferences: a shut tree and a
    // selected folder inside it.
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::SetAccountExpanded {
        account: AccountId::try_from("work").unwrap(),
        expanded: false,
    })
    .await
    .unwrap();
    assert!(app
        .mailbox_list()
        .account_folders
        .iter()
        .any(|row| row.account_id.as_str() == "work"));

    remove_work(&app).await;

    assert_eq!(ops.removals(), vec!["work".to_owned()]);
    assert!(
        app.settings()
            .accounts
            .iter()
            .all(|row| row.account.as_str() != "work"),
        "the settings rows lost the account"
    );
    assert!(
        app.mailbox_list()
            .account_folders
            .iter()
            .all(|row| row.account_id.as_str() != "work"),
        "the pane lost the account's tree"
    );
    assert_eq!(
        app.settings().error,
        None,
        "a removal that landed answers no error"
    );

    // The engine's domains for the account went with the config.
    assert!(app
        .calendar()
        .calendars
        .iter()
        .all(|calendar| calendar.account.as_str() != "work"));
    assert!(
        app.people().create_targets.is_empty(),
        "the contacts leg and its domain went"
    );

    // The preferences' account-scoped entries went, and the selection that
    // pointed inside the account returned to the unified view.
    let persisted = load_preferences(dir.join("preferences.toml"));
    assert!(persisted.account_expanded("work"), "no shut tree remains");
    assert_eq!(persisted.selected_folder, None);
    assert_eq!(app.scope_for_test(), None, "the unified view stands");
}

#[tokio::test]
async fn removing_the_account_clears_its_open_reading_and_row_sentences() {
    let dir = scratch("open-reading");
    let (app, _surfaces, ops) = two_accounts(&dir).await;
    // The account holds a row sentence: a save the store refused.
    ops.failing_dial("work", "the origin could not be reached");
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: AccountId::try_from("work").unwrap(),
        section: Some(section()),
    }))
    .await
    .unwrap();
    assert!(
        !app.dav_errors_for_test().is_empty(),
        "the row holds the refusal first"
    );
    // And an open message inside the account, once the mail half syncs its
    // one invitation message.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::OpenMessage {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    assert!(app.reading().is_some(), "the message is open");

    remove_work(&app).await;

    assert!(
        app.reading().is_none(),
        "the pane no longer names a message of a purged account"
    );
    assert!(
        app.dav_errors_for_test().is_empty(),
        "the row's sentence went with the row"
    );
}

#[tokio::test]
async fn a_failed_removal_answers_typed_and_nothing_goes() {
    let dir = scratch("failed");
    let (app, _surfaces, ops) = two_accounts(&dir).await;
    ops.failing_removal();

    remove_work(&app).await;

    assert_eq!(
        app.settings().error,
        Some(SettingsError::RemoveFailed),
        "the refusal is typed on the snapshot"
    );
    assert_eq!(
        ops.removals(),
        vec!["work".to_owned()],
        "the host was asked"
    );
    assert!(
        app.settings()
            .accounts
            .iter()
            .any(|row| row.account.as_str() == "work"),
        "the account keeps everything it had"
    );
    assert_eq!(
        ops.stored_section("work"),
        Some("ada".to_owned()),
        "the stored section stands"
    );

    // The script is one shot: the next ask lands, and a write that lands
    // clears the refusal.
    remove_work(&app).await;
    assert_eq!(app.settings().error, None);
    assert!(app
        .settings()
        .accounts
        .iter()
        .all(|row| row.account.as_str() != "work"));
}
