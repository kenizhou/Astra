// SPDX-License-Identifier: GPL-3.0-only
//! The gated move-to-folder test against the live harness: one seeded note
//! moved into a created folder through the product's own batch move, read
//! back in the destination on the next refresh, and withdrawn to the INBOX
//! again with the folder deleted at cleanup, so the harness keeps its
//! three-note contract.

use crate::{FolderIntent, FolderRef, Intent};

use super::{
    folder_proof::{folder_row_by_name, note_ref, sender_app, withdraw_proof_folders, MOVED},
    harness, inbox_key,
};

/// The move-to-folder batch against the live harness: the row menu's own
/// verb over the real dial, one intent carrying one row and one destination.
#[tokio::test]
async fn a_move_to_folder_batch_moves_a_seeded_note_and_withdraws_it_back() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, account) = sender_app(&host, &port, &user, &password).await;

    // The same converging sweep the lifecycle proof opens with.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    withdraw_proof_folders(&app, &account).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::Folders(FolderIntent::Create {
        account: account.clone(),
        parent: None,
        name: MOVED.to_owned(),
    }))
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let created = folder_row_by_name(&app, &account, MOVED);

    // The move: the note rides in through the batch the row menu dispatches.
    let note = note_ref(&app, &account, "Harness note two").await;
    let destination = FolderRef::from_parts(account.as_str(), created.key.as_str().to_owned())
        .expect("the destination folder reference");
    app.dispatch(Intent::MoveMessages {
        rows: vec![note],
        folder: destination.clone(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::SelectFolder {
        folder: destination,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .any(|row| row.subject == "Harness note two"),
        "the moved note reads back in the destination on the next refresh"
    );

    // The withdrawal: the note rides home, the folder goes for good, and the
    // INBOX keeps its three-note contract.
    let note = note_ref(&app, &account, "Harness note two").await;
    let inbox = FolderRef::from_parts(account.as_str(), inbox_key(&app, &account))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::MoveMessages {
        rows: vec![note],
        folder: inbox,
    })
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
