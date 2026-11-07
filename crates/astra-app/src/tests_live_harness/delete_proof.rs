// SPDX-License-Identifier: GPL-3.0-only
//! The gated permanent-delete proof against the live harness: the row's
//! irreversible verb observed over the real server. Two distinguishable notes
//! are submitted from astra1 through the engine's own SMTP leg (the same leg
//! the invitation suite seeds over) so they land in astra2's INBOX, the row is
//! opened, the delete intent is dispatched, and the deleted copy is gone from
//! the account's stored set with no folder holding it, the harness Trash
//! included (the verb is the permanent delete, never a move), while the other
//! note stands intact. The offline suite owns the shape pins (the engine's
//! delete edit, never a trash move; the batch exclusion); this proof owns the
//! real-server landing. The cleanup withdraws the survivor and the sender's
//! filed copies through the engine's own delete, so the harness keeps the
//! shape the mail suites pin.

use astra_viewmodel::{FolderRole, RowKey};
use engine_api::{AccountId, Provider};

use crate::{reference::RowRef, App, FolderRef, Intent, MessageRef};

use super::{harness, inbox_key, proof, row_by_subject};

/// The subject of the note the proof deletes: unique among the harness fixtures,
/// so a row carrying it is this proof's mail and nothing the seed delivered.
const GONE: &str = "Harness delete proof gone";

/// The subject of the note that must survive the delete: unique among the
/// fixtures the same way.
const KEPT: &str = "Harness delete proof kept";

/// The words both seeded notes carry.
const WORDS: &str = "Seeded for the permanent delete proof.";

/// The folders the pane lists for `account`: every key the proof walks to show
/// no folder holds the deleted copy.
fn folder_keys(app: &App<Box<dyn Provider>>, account: &AccountId) -> Vec<(String, bool)> {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .expect("the pane lists the account's folders")
        .folders
        .iter()
        .map(|folder| {
            (
                folder.key.as_str().to_owned(),
                folder.role == Some(FolderRole::Trash),
            )
        })
        .collect()
}

/// The permanent delete observed over the live harness: a row's verb removes
/// the mail for good, the stored set and every folder (Trash included) hold no
/// copy, the untouched note stands, and the run leaves the harness as it found
/// it.
#[tokio::test]
async fn the_delete_verb_removes_the_mail_for_good_over_the_live_store() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // Back to a known state: a run interrupted before its cleanup may have left
    // the proof mail behind; the sweep withdraws every copy of both subjects.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[GONE, KEPT]).await;
    proof::delete_subject_mail(&app, &recipient, &[GONE, KEPT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The seeding: two distinguishable notes submitted through the engine's own
    // SMTP leg from astra1, delivered into astra2's INBOX.
    let handle = app
        .account_handle(&sender)
        .await
        .expect("the sending account");
    let provider = handle.providers.first().expect("the submission provider");
    for subject in [GONE, KEPT] {
        let draft = engine_api::Draft::new(
            crate::drafts::mint_message_id().expect("a fresh message id"),
            engine_api::EmailAddress::new(&user),
            vec![engine_api::EmailAddress::new(proof::RECIPIENT)],
            subject.to_owned(),
            WORDS,
        );
        app.engine
            .submit_mail(provider, &sender, &draft)
            .await
            .expect("the seeding submission");
    }
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // Both notes stand in the recipient's INBOX, the rows the proof acts on.
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let gone_row = row_by_subject(&snapshot, GONE).key.clone();
    row_by_subject(&snapshot, KEPT);

    // The row is opened, the way the verb reaches it from the reading pane.
    let RowKey::Message { account, key } = gone_row else {
        panic!("a flat list row is keyed by its message");
    };
    let row = MessageRef {
        account: account.clone(),
        key: key.clone(),
    };
    app.dispatch(Intent::OpenMessage {
        message: row.clone(),
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, GONE);
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The verb: the permanent delete through the product's own dispatch, then
    // the passes that land and observe it.
    app.dispatch(Intent::DeleteMessage {
        row: RowRef::Message(row),
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // Gone from the account's stored set: no stored message carries the note.
    let stored = app.engine.messages(&recipient).await.unwrap();
    assert!(
        stored
            .iter()
            .all(|message| message.envelope.subject.as_deref() != Some(GONE)),
        "the deleted note is gone from the account's stored set"
    );

    // No folder holds it: every folder the pane lists, the Trash among them.
    let folders = folder_keys(&app, &recipient);
    let mut saw_trash = false;
    for (key, trash) in &folders {
        saw_trash |= *trash;
        let folder =
            FolderRef::from_parts(recipient.as_str(), key.clone()).expect("the folder reference");
        app.dispatch(Intent::SelectFolder { folder }).await.unwrap();
        let snapshot = app.mailbox_list();
        assert!(
            snapshot.rows.iter().all(|row| row.subject != GONE),
            "no folder the pane lists holds the deleted note ({key})"
        );
    }
    assert!(
        saw_trash,
        "the walk covered the account's Trash folder, the destination a move would have used"
    );

    // The other note stands intact: still in the INBOX, still in the store.
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let kept = row_by_subject(&app.mailbox_list(), KEPT).key.clone();
    let RowKey::Message { key, .. } = kept else {
        panic!("a flat list row is keyed by its message");
    };
    assert!(
        app.engine
            .messages(&recipient)
            .await
            .unwrap()
            .iter()
            .any(|message| message.id.key() == &key),
        "the untouched note is still in the account's stored set"
    );

    // The cleanup: the survivor and the sender's filed copies withdrawn, the
    // harness keeping the shape the other suites pin.
    proof::delete_subject_mail(&app, &recipient, &[KEPT]).await;
    proof::delete_subject_mail(&app, &sender, &[GONE, KEPT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot
            .rows
            .iter()
            .all(|row| row.subject != GONE && row.subject != KEPT),
        "the recipient's INBOX holds neither proof note at the end"
    );
}
