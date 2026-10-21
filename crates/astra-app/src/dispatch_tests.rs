// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the dispatch loop: the intent-to-use-case table, the snapshot it
//! rebuilds, the window it grows, the preferences it writes through, and the boot
//! sequence that assembles the whole runtime. The fixtures live in `dispatch_fakes`;
//! the message-list tests live in `dispatch_list_tests`.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use engine_api::{AccountId, MailboxRole};

use crate::{boot, dispatch_fakes as fakes, reference::FolderRef, Intent, Surface, ViewMode};

#[path = "dispatch_list_tests.rs"]
mod list;

#[path = "dispatch_unified_tests.rs"]
mod unified;

use fakes::{account, app, app_with_prefs, folder, inbox, key, scratch, NoConnector};

#[tokio::test]
async fn refresh_mail_fans_out_sync_and_publishes_the_mailbox_list() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut unread_inbox = inbox();
    unread_inbox.unread_count = Some(4);
    let provider = fakes::FakeProvider::with(vec![
        unread_inbox,
        folder("archive", "Archive"),
        folder("clients", "Clients"),
    ]);
    let syncs = Arc::clone(&provider.syncs);
    let app = app(vec![account("work", provider)], &surfaces);

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(
        fakes::pane_rows(&snapshot),
        3,
        "every folder the pass listed"
    );
    let work = &snapshot.account_folders[0];
    assert_eq!(work.display_name, "me@work.local");
    assert_eq!(
        fakes::folder_keys(&snapshot, "work"),
        ["inbox", "archive", "clients"]
    );
    assert_eq!(work.folders[0].unread, 4, "the server's own unread count");
    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the rebuilt list is signalled"
    );
    assert!(syncs.load(Ordering::SeqCst) >= 1, "the account synced");
}

#[tokio::test]
async fn refresh_mail_syncs_every_account() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let first = fakes::FakeProvider::with(vec![inbox()]);
    let second = fakes::FakeProvider::with(vec![inbox(), folder("archive", "Archive")]);
    let counts = (Arc::clone(&first.syncs), Arc::clone(&second.syncs));
    let app = app(
        vec![account("one", first), account("two", second)],
        &surfaces,
    );

    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert!(counts.0.load(Ordering::SeqCst) >= 1, "the first synced");
    assert!(counts.1.load(Ordering::SeqCst) >= 1, "the second synced");
    assert_eq!(app.mailbox_list().account_folders.len(), 2);
}

#[tokio::test]
async fn the_account_rows_carry_their_sending_address() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![inbox()]);
    let app = app(vec![account("work", provider)], &surfaces);

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let work = &app.mailbox_list().account_folders[0];
    assert_eq!(
        work.address, "me@work.local",
        "the row the From picker reads carries the address mail leaves from"
    );
}

#[tokio::test]
async fn select_folder_persists_to_preferences_and_rebuilds_the_list() {
    let dir = scratch("select-folder");
    let prefs_path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_prefs(
        vec![account("work", fakes::FakeProvider::with(vec![inbox()]))],
        Some(prefs_path.clone()),
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let signals = surfaces.lock().unwrap().len();

    let selected = FolderRef::from_parts("work", "archive".to_owned()).unwrap();
    app.dispatch(Intent::SelectFolder { folder: selected })
        .await
        .unwrap();

    assert!(
        surfaces.lock().unwrap().len() > signals,
        "the rebuilt list is republished"
    );
    let stored = crate::load_preferences(&prefs_path);
    let selected = stored.selected_folder.expect("the folder persisted");
    assert_eq!(selected.account.as_str(), "work");
    assert_eq!(selected.key, "archive");
}

#[tokio::test]
async fn the_persisted_selection_seeds_a_fresh_app_over_the_same_file() {
    let dir = scratch("selection-relaunch");
    let prefs_path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_prefs(
        vec![account("work", fakes::FakeProvider::with(vec![inbox()]))],
        Some(prefs_path.clone()),
        &surfaces,
    );
    let selected = FolderRef::from_parts("work", "archive".to_owned()).unwrap();
    app.dispatch(Intent::SelectFolder { folder: selected })
        .await
        .unwrap();

    // A relaunch: a fresh app over the same preferences file reopens the folder.
    let relaunched = app_with_prefs(
        vec![account("work", fakes::FakeProvider::with(vec![inbox()]))],
        Some(prefs_path),
        &surfaces,
    );
    assert_eq!(
        relaunched.scope_for_test(),
        crate::Scope::Folder(FolderRef::from_parts("work", "archive".to_owned()).unwrap()),
        "a relaunch reopens the persisted selection"
    );
}

#[tokio::test]
async fn select_account_scopes_the_list_and_resets_the_selected_folder() {
    let dir = scratch("select-account");
    let prefs_path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_prefs(
        vec![
            account("work", fakes::FakeProvider::with(vec![inbox()])),
            account("home", fakes::FakeProvider::with(vec![inbox()])),
        ],
        Some(prefs_path.clone()),
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let selected = FolderRef::from_parts("work", "archive".to_owned()).unwrap();
    app.dispatch(Intent::SelectFolder { folder: selected })
        .await
        .unwrap();

    app.dispatch(Intent::SelectAccount(Some(
        AccountId::try_from("home").unwrap(),
    )))
    .await
    .unwrap();

    // The pane contract: selecting an account never empties the others' trees.
    let snapshot = app.mailbox_list();
    assert_eq!(snapshot.account_folders.len(), 2);
    assert!(snapshot
        .account_folders
        .iter()
        .any(|row| row.account_id.as_str() == "work"));
    assert!(
        crate::load_preferences(&prefs_path)
            .selected_folder
            .is_none(),
        "an account selection resets the selected folder"
    );
}

#[tokio::test]
async fn expansion_intents_write_through_to_the_pane_and_the_preferences_file() {
    let dir = scratch("expansion");
    let prefs_path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut parent = folder("archive", "Archive");
    parent.role = Some(MailboxRole::Archive);
    let mut child = folder("archive/2024", "2024");
    child.parent = Some(key("archive"));
    let app = app_with_prefs(
        vec![account(
            "work",
            fakes::FakeProvider::with(vec![inbox(), parent, child]),
        )],
        Some(prefs_path.clone()),
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::SetAccountExpanded {
        account: AccountId::try_from("work").unwrap(),
        expanded: false,
    })
    .await
    .unwrap();
    app.dispatch(Intent::SetFolderExpanded {
        folder: FolderRef::from_parts("work", "archive".to_owned()).unwrap(),
        expanded: false,
    })
    .await
    .unwrap();

    let snapshot = app.mailbox_list();
    let work = &snapshot.account_folders[0];
    assert!(!work.expanded, "the account tree is shut");
    let archive = work
        .folders
        .iter()
        .find(|row| row.key == key("archive"))
        .unwrap();
    assert!(!archive.expanded, "the folder's own tree is shut");
    let stored = crate::load_preferences(&prefs_path);
    assert!(!stored.account_expanded("work"));
    assert!(!stored.folder_expanded("work", "archive"));
}

#[tokio::test]
async fn set_view_mode_persists_and_resets_the_window() {
    let dir = scratch("view-mode");
    let prefs_path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_prefs(
        vec![account("work", fakes::FakeProvider::with(vec![inbox()]))],
        Some(prefs_path.clone()),
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::ShowMore).await.unwrap();
    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();

    assert_eq!(
        crate::load_preferences(&prefs_path).view_mode,
        ViewMode::Threaded,
        "the grouping survives a restart"
    );
    assert_eq!(app.visible_limit(), crate::PAGE, "the window reset");
    assert_eq!(app.view_mode(), ViewMode::Threaded);
}

#[tokio::test]
async fn show_more_grows_the_window_over_the_pane_rows() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let many = |prefix: &str| {
        (0..60)
            .map(|index| folder(&format!("{prefix}{index:02}"), &format!("Folder {index}")))
            .collect::<Vec<_>>()
    };
    let mut first = many("a");
    let mut second = many("b");
    first.insert(0, inbox());
    second.insert(0, inbox());
    let total = first.len() + second.len();
    let app = app(
        vec![
            account("work", fakes::FakeProvider::with(first)),
            account("home", fakes::FakeProvider::with(second)),
        ],
        &surfaces,
    );

    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        fakes::pane_rows(&app.mailbox_list()),
        crate::PAGE,
        "the first page bounds the pane"
    );

    app.dispatch(Intent::ShowMore).await.unwrap();
    assert_eq!(
        fakes::pane_rows(&app.mailbox_list()),
        total,
        "one more page reaches every row"
    );
    assert_eq!(app.visible_limit(), crate::PAGE * 2);
}

#[tokio::test]
async fn a_superseded_rebuild_never_publishes() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(
        vec![account("work", fakes::FakeProvider::with(vec![inbox()]))],
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // An older rebuild's generation, handed in directly: the race itself, not an
    // attempt to win a scheduling coin-toss.
    let superseded = app.list_generation();
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    let signals = surfaces.lock().unwrap().len();
    let rows = fakes::pane_rows(&app.mailbox_list());

    app.rebuild_snapshot_for(superseded).await;

    assert_eq!(
        fakes::pane_rows(&app.mailbox_list()),
        rows,
        "a rebuild a newer intent superseded must not repaint the list"
    );
    assert_eq!(
        surfaces.lock().unwrap().len(),
        signals,
        "a superseded rebuild must not publish at all"
    );

    // The guard drops what is stale, not what is slow.
    app.rebuild_snapshot_for(app.list_generation()).await;
    assert_eq!(
        surfaces.lock().unwrap().len(),
        signals + 1,
        "a rebuild nothing superseded publishes normally"
    );
}

#[tokio::test]
async fn a_sync_invalidates_the_row_cache() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(
        vec![account("work", fakes::FakeProvider::with(vec![inbox()]))],
        &surfaces,
    );
    app.rebuild_snapshot().await;
    assert!(app.list_cache_is_loaded());

    let before = app.row_cache_generation();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert_ne!(
        app.row_cache_generation(),
        before,
        "a sync bumps the cache generation"
    );
    assert!(
        app.list_cache_is_loaded(),
        "the post-sync rebuild reloads and recaches"
    );
}

#[tokio::test]
async fn add_new_account_deferred_lists_without_syncing_until_told() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![inbox(), folder("archive", "Archive")]);
    let syncs = Arc::clone(&provider.syncs);
    let app = app(Vec::new(), &surfaces);

    app.add_new_account_deferred(account("work", provider))
        .await;

    let snapshot = app.mailbox_list();
    assert_eq!(snapshot.account_folders.len(), 1, "listed at once");
    assert_eq!(
        fakes::pane_rows(&snapshot),
        0,
        "nothing is synced, so no folders are stored yet"
    );
    assert_eq!(syncs.load(Ordering::SeqCst), 0, "no sync ran");

    app.sync_added_account(&AccountId::try_from("work").unwrap())
        .await;
    assert!(syncs.load(Ordering::SeqCst) >= 1, "the first sync ran");
    assert_eq!(
        fakes::pane_rows(&app.mailbox_list()),
        2,
        "the synced tree is on screen"
    );
}

#[tokio::test]
async fn boot_primes_the_cached_list_and_spawns_each_accounts_first_sync() {
    let dir = scratch("boot");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(AtomicUsize::new(0));
    let observer = Arc::new(fakes::RecordingObserver {
        surfaces: Arc::clone(&surfaces),
    });

    let app = boot(
        &dir.join("store.db"),
        vec![
            account("work", fakes::FakeProvider::with(vec![inbox()])),
            account("home", fakes::FakeProvider::with(vec![inbox()])),
        ],
        Some(Box::new(NoConnector)),
        observer,
        {
            let spawned = Arc::clone(&spawned);
            move |task: Pin<Box<dyn Future<Output = ()> + Send>>| {
                drop(task);
                spawned.fetch_add(1, Ordering::SeqCst);
            }
        },
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the primed list is published before any sync"
    );
    assert_eq!(
        app.mailbox_list().account_folders.len(),
        2,
        "every account lists from the store"
    );
    assert_eq!(
        spawned.load(Ordering::SeqCst),
        5,
        "each account's first sync, the autosave driver, the AI pass driver, and the \
         reply-drafting driver are spawned, not awaited",
    );
}
