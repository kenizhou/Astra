// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the folder-tree writes: each intent verb mapped onto the engine's
//! outbox-mediated edit over a writable fake, the capability gate that refuses
// a provider which cannot change its tree, and the re-selection that follows a
//! folder whose key an edit minted afresh. The fixtures live in
//! `dispatch_fakes`; the tree-mutating half of the fake lives in its `tree`
//! child.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::{
    dispatch_fakes as fakes, folder_writes::FolderWriteError, reference::FolderRef, FolderIntent,
    Intent, Scope, Surface,
};

use fakes::{account, app, folder, inbox, trash};

/// An app over one tree-writing account, synced once, plus the provider's edit
/// log: the shared prefix of every folder-write test.
async fn writable_tree(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    mailboxes: Vec<engine_api::Mailbox>,
) -> (
    crate::App<fakes::FakeProvider>,
    Arc<Mutex<Vec<engine_provider::MailboxEdit>>>,
) {
    let provider = fakes::FakeProvider::with(mailboxes).with_tree_writes();
    let edits = Arc::clone(&provider.mailbox_edits);
    let app = app(vec![account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, edits)
}

/// The work account id, the way every folder-write test names one.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

#[tokio::test]
async fn a_folder_create_lands_in_the_pane_through_the_engine() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, edits) = writable_tree(&surfaces, vec![inbox(), folder("archive", "Archive")]).await;

    app.dispatch(Intent::Folders(FolderIntent::Create {
        account: work(),
        parent: None,
        name: "Records".to_owned(),
    }))
    .await
    .unwrap();

    assert_eq!(
        fakes::folder_keys(&app.mailbox_list(), "work"),
        ["inbox", "archive", "Records"],
        "the new folder is drawn in the pane"
    );
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_provider::MailboxEdit::Create {
            name: "Records".to_owned(),
            parent: None,
        }],
        "one create through the provider, exactly as the engine's edit_mailbox sends it"
    );
}

#[tokio::test]
async fn a_nested_create_lands_inside_the_named_parent() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(&surfaces, vec![inbox(), folder("archive", "Archive")]).await;

    app.dispatch(Intent::Folders(FolderIntent::Create {
        account: work(),
        parent: Some("archive".to_owned()),
        name: "2026".to_owned(),
    }))
    .await
    .unwrap();

    assert_eq!(
        fakes::folder_keys(&app.mailbox_list(), "work"),
        ["inbox", "archive", "archive/2026"],
        "the folder lists inside its parent, keyed by its path"
    );
}

#[tokio::test]
async fn a_folder_rename_reselects_under_the_receipts_key() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(&surfaces, vec![inbox(), folder("clients", "Clients")]).await;
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "clients".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    app.dispatch(Intent::Folders(FolderIntent::Rename {
        account: work(),
        folder: "clients".to_owned(),
        name: "Customers".to_owned(),
    }))
    .await
    .unwrap();

    assert_eq!(
        fakes::folder_keys(&app.mailbox_list(), "work"),
        ["inbox", "Customers"],
        "the pane draws the renamed folder under its minted key"
    );
    assert_eq!(
        app.scope_for_test(),
        Scope::Folder(FolderRef::from_parts("work", "Customers".to_owned()).unwrap()),
        "the selection follows the receipt's key"
    );
}

#[tokio::test]
async fn a_folder_move_reparents_it_and_reselects() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(
        &surfaces,
        vec![
            inbox(),
            folder("archive", "Archive"),
            folder("clients", "Clients"),
            folder("clients/acme", "Acme"),
        ],
    )
    .await;
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "clients".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    app.dispatch(Intent::Folders(FolderIntent::Move {
        account: work(),
        folder: "clients".to_owned(),
        parent: Some("archive".to_owned()),
    }))
    .await
    .unwrap();

    assert_eq!(
        fakes::folder_keys(&app.mailbox_list(), "work"),
        [
            "inbox",
            "archive",
            "archive/Clients",
            "archive/Clients/acme"
        ],
        "the folder and its subtree moved under the new parent, keeping its name"
    );
    assert_eq!(
        app.scope_for_test(),
        Scope::Folder(FolderRef::from_parts("work", "archive/Clients".to_owned()).unwrap()),
        "the selection follows the moved folder's minted key"
    );
}

#[tokio::test]
async fn a_folder_trash_files_it_into_the_trash() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(
        &surfaces,
        vec![inbox(), trash(), folder("clients", "Clients")],
    )
    .await;

    app.dispatch(Intent::Folders(FolderIntent::Trash {
        account: work(),
        folder: "clients".to_owned(),
    }))
    .await
    .unwrap();

    let keys = fakes::folder_keys(&app.mailbox_list(), "work");
    assert!(
        keys.iter().any(|key| key.starts_with("trash/Clients")),
        "the folder landed inside the Trash, keeping its name: {keys:?}"
    );
    assert!(
        !keys.contains(&"clients".to_owned()),
        "the folder no longer sits at its old place"
    );
}

#[tokio::test]
async fn a_folder_delete_removes_the_tree_and_the_selection_falls_back() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(
        &surfaces,
        vec![inbox(), trash(), folder("clients", "Clients")],
    )
    .await;
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "clients".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    app.dispatch(Intent::Folders(FolderIntent::Delete {
        account: work(),
        folder: "clients".to_owned(),
    }))
    .await
    .unwrap();

    assert_eq!(
        fakes::folder_keys(&app.mailbox_list(), "work"),
        ["inbox", "trash"],
        "the folder is gone for good"
    );
    assert_eq!(
        app.scope_for_test(),
        Scope::Account(work()),
        "the selection fell back to the account view"
    );
}

#[tokio::test]
async fn a_provider_without_tree_writes_is_refused_typed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![inbox()]);
    let edits = Arc::clone(&provider.mailbox_edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let outcome = app
        .write_folder(FolderIntent::Create {
            account: work(),
            parent: None,
            name: "Records".to_owned(),
        })
        .await;

    assert_eq!(outcome, Err(FolderWriteError::NotCapable));
    assert!(
        edits.lock().unwrap().is_empty(),
        "nothing reached the provider"
    );
    assert_eq!(
        fakes::folder_keys(&app.mailbox_list(), "work"),
        ["inbox"],
        "the pane is unchanged"
    );
}

#[tokio::test]
async fn a_folder_key_the_tree_does_not_hold_is_refused_typed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(&surfaces, vec![inbox()]).await;

    let outcome = app
        .write_folder(FolderIntent::Rename {
            account: work(),
            folder: "ghost".to_owned(),
            name: "Anything".to_owned(),
        })
        .await;

    assert_eq!(outcome, Err(FolderWriteError::UnknownFolder));
}

#[tokio::test]
async fn a_delete_of_a_key_the_tree_does_not_hold_is_refused_typed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = writable_tree(&surfaces, vec![inbox()]).await;

    let outcome = app
        .write_folder(FolderIntent::Delete {
            account: work(),
            folder: "ghost".to_owned(),
        })
        .await;

    assert_eq!(outcome, Err(FolderWriteError::UnknownFolder));
}
