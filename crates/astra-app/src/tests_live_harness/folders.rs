// SPDX-License-Identifier: GPL-3.0-only
//! The gated folder lifecycle test against the live harness: the folder leg
//! end to end over the IMAP dial, every step through the product's own
//! `write_folder` path.
//!
//! The proof creates a parent folder at the top of the tree and the proof
//! folder nested inside it, renames it (the pane re-selecting under the
//! receipt's minted key, message rows still rendering once the next mail
//! sync lands), reparents it to the top, trashes it under the account's
//! Trash, and deletes it for good with the selection falling back. A seeded
//! note rides into the folder and home again through the product's own
//! batch move, so the rows the rename must still render stand on the real
//! wire and the harness keeps its three-note contract: the note rides home
//! before any folder is trashed or deleted, because the delete is permanent.

use crate::{FolderIntent, FolderRef, Intent, Scope};

use super::{
    folder_proof::{
        folder_row_by_name, note_ref, sender_app, under_trash, withdraw_proof_folders, NESTED,
        PARENT, RENAMED,
    },
    harness, inbox_key,
};

/// The folder lifecycle against the live harness: created nested, renamed
/// under a minted key with its message rows still rendering after the next
/// sync, reparented, trashed under the account's Trash, and deleted for
/// good with the selection falling back, every step through the product's
/// own write path over the real dial, cleaned up after itself.
#[tokio::test]
async fn the_folder_lifecycle_runs_end_to_end_over_the_imap_dial_and_cleans_up_after_itself() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, account) = sender_app(&host, &port, &user, &password).await;

    // Back to a known state: a run interrupted before its cleanup may have
    // left its folders behind, with a seeded note stuck inside one. The first
    // pass presents what the store holds, the sweep rescues and deletes, and
    // the next passes land and observe the deletes.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    withdraw_proof_folders(&app, &account).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The create: a parent at the top of the tree, then the proof folder
    // nested inside it.
    app.dispatch(Intent::Folders(FolderIntent::Create {
        account: account.clone(),
        parent: None,
        name: PARENT.to_owned(),
    }))
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let parent = folder_row_by_name(&app, &account, PARENT);
    assert_eq!(
        parent.parent, None,
        "the parent stands at the top of the tree"
    );
    assert_eq!(parent.depth, 0, "the parent draws at the top of the tree");

    app.dispatch(Intent::Folders(FolderIntent::Create {
        account: account.clone(),
        parent: Some(parent.key.as_str().to_owned()),
        name: NESTED.to_owned(),
    }))
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let nested = folder_row_by_name(&app, &account, NESTED);
    assert_eq!(
        nested.parent.as_ref(),
        Some(&parent.key),
        "the created folder is nested under its parent"
    );
    assert_eq!(nested.depth, 1, "the nested folder draws one level down");

    // A seeded note rides in, so the folder holds a message row over the real
    // dial: the row the rename's rendering contract stands on.
    let note = note_ref(&app, &account, "Harness note one").await;
    let nested_ref = FolderRef::from_parts(account.as_str(), nested.key.as_str().to_owned())
        .expect("the nested folder reference");
    app.dispatch(Intent::SelectFolder {
        folder: nested_ref.clone(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::MoveMessages {
        rows: vec![note],
        folder: nested_ref,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .any(|row| row.subject == "Harness note one"),
        "the moved note reads back in the destination folder"
    );

    // The rename: the pane re-selects the folder under the receipt's minted
    // key, and the message rows still render once the next mail sync lands.
    let old_key = nested.key.as_str().to_owned();
    app.dispatch(Intent::Folders(FolderIntent::Rename {
        account: account.clone(),
        folder: old_key.clone(),
        name: RENAMED.to_owned(),
    }))
    .await
    .unwrap();
    let Scope::Folder(selected) = app.scope_for_test() else {
        panic!("the rename keeps a folder selected");
    };
    assert_eq!(
        selected.account, account,
        "the re-selection stays on the account"
    );
    assert_ne!(
        selected.key, old_key,
        "the selection follows the receipt's minted key, not the folder's old one"
    );
    let minted = selected.key.clone();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let renamed = folder_row_by_name(&app, &account, RENAMED);
    assert_eq!(
        renamed.key.as_str(),
        minted,
        "the synced tree holds the folder under the key the receipt minted"
    );
    assert_eq!(
        renamed.parent.as_ref(),
        Some(&parent.key),
        "the rename keeps the folder nested where it was"
    );
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .any(|row| row.subject == "Harness note one"),
        "message rows still render after the rename once the next mail sync lands"
    );

    // The reparent: the folder moves to the top of the tree, the selection
    // following its next minted key.
    app.dispatch(Intent::Folders(FolderIntent::Move {
        account: account.clone(),
        folder: renamed.key.as_str().to_owned(),
        parent: None,
    }))
    .await
    .unwrap();
    let Scope::Folder(selected) = app.scope_for_test() else {
        panic!("the move keeps a folder selected");
    };
    let reparented_key = selected.key.clone();
    assert_ne!(
        reparented_key, minted,
        "the reparent mints its own key and the selection follows it"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let reparented = folder_row_by_name(&app, &account, RENAMED);
    assert_eq!(
        reparented.key.as_str(),
        reparented_key,
        "the synced tree holds the reparented folder under its minted key"
    );
    assert_eq!(
        reparented.parent, None,
        "the reparented folder stands at the top of the tree"
    );

    // The note rides home before the folder leaves: the delete that ends this
    // leg is permanent, and the harness keeps its three-note contract.
    let note = note_ref(&app, &account, "Harness note one").await;
    let inbox = FolderRef::from_parts(account.as_str(), inbox_key(&app, &account))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::MoveMessages {
        rows: vec![note],
        folder: inbox,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != "Harness note one"),
        "the folder holds nothing once the note is home"
    );

    // The trash: the folder, still selected, lands under the account's Trash.
    app.dispatch(Intent::Folders(FolderIntent::Trash {
        account: account.clone(),
        folder: reparented_key,
    }))
    .await
    .unwrap();
    let Scope::Folder(selected) = app.scope_for_test() else {
        panic!("the trash keeps the folder selected under its new key");
    };
    let trashed_key = selected.key.clone();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let trashed = folder_row_by_name(&app, &account, RENAMED);
    assert_eq!(
        trashed.key.as_str(),
        trashed_key,
        "the selection follows the key the trash minted"
    );
    assert!(
        under_trash(&app, &account, &trashed_key),
        "the trashed folder lands under the account's Trash"
    );

    // The permanent delete: the tree is gone and the selection falls back to
    // the account's own view.
    app.dispatch(Intent::Folders(FolderIntent::Delete {
        account: account.clone(),
        folder: trashed_key,
    }))
    .await
    .unwrap();
    assert_eq!(
        app.scope_for_test(),
        Scope::Account(account.clone()),
        "the selection falls back to the account's own view once the folder goes"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The parent goes too, and the sweep lands whatever a step above left.
    app.dispatch(Intent::Folders(FolderIntent::Delete {
        account: account.clone(),
        folder: parent.key.as_str().to_owned(),
    }))
    .await
    .unwrap();
    withdraw_proof_folders(&app, &account).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let inbox = FolderRef::from_parts(account.as_str(), inbox_key(&app, &account))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        ["Harness note three", "Harness note two", "Harness note one"],
        "the INBOX is the three seeded notes again, newest first"
    );
}
