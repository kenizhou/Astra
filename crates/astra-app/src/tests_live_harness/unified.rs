// SPDX-License-Identifier: GPL-3.0-only
//! The gated unified-rows test against the live harness: an app over both
//! harness accounts, on the all-inboxes scope, lists rows of both accounts'
//! inboxes newest first, and only inboxes. A fresh note from astra1 to
//! astra2 puts a known row on the second account's side beside the seeds on
//! the first's, and the run withdraws both copies so the harness keeps its
//! seeded shape.

use astra_viewmodel::RowKey;

use crate::{FolderRef, Intent};

use super::proof;
use super::{harness, inbox_key, row_by_subject};

/// The subject the unified proof's fresh note carries, unique among the
/// harness fixtures.
const SUBJECT: &str = "Harness unified proof";

/// The words the fresh note carries.
const WORDS: &str = "The note whose row stands on the second account's inbox.";

/// The unified rows against the live harness: both accounts' inbox rows
/// merged newest first on the all-inboxes scope, both accounts standing in
/// the one list, nothing from outside an inbox, cleaned up after itself.
#[tokio::test]
async fn the_unified_list_holds_both_accounts_inbox_rows_newest_first() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // The same converging sweep the delivery proofs open with, over this
    // proof's own subject.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // A known row stands on the second account's side: the seed leaves
    // astra2 nothing, so the proof submits its own note there.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: proof::document(WORDS),
        blobs: Vec::new(),
    })
    .await
    .unwrap();

    // The unified list: both accounts' inbox rows merged, newest first. The
    // fresh note is the newest row, the three seeds follow in their pinned
    // order, and the Projects note stays out: the scope holds inboxes only.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::SelectAccount(None)).await.unwrap();
    let snapshot = app.mailbox_list();
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        [
            SUBJECT,
            "Harness note three",
            "Harness note two",
            "Harness note one"
        ],
        "the unified list merges both accounts' inbox rows, newest first"
    );
    let RowKey::Message {
        account: fresh_account,
        ..
    } = row_by_subject(&snapshot, SUBJECT).key.clone()
    else {
        panic!("a unified row is keyed by its message");
    };
    assert_eq!(
        fresh_account, recipient,
        "the fresh note's row is the receiving account's"
    );
    let RowKey::Message {
        account: seeded_account,
        ..
    } = row_by_subject(&snapshot, "Harness note one").key.clone()
    else {
        panic!("a unified row is keyed by its message");
    };
    assert_eq!(
        seeded_account, sender,
        "the seeded notes' rows are the sending account's"
    );
    assert!(
        snapshot
            .rows
            .iter()
            .all(|row| row.subject != "Harness rich note"),
        "the Projects note is no unified row: the scope holds inboxes only"
    );

    // The cleanup: both copies of the fresh note are withdrawn, and the
    // harness keeps its seeded shape.
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    assert!(
        app.mailbox_list().rows.is_empty(),
        "astra2's INBOX holds nothing once the proof is withdrawn"
    );
}
