// SPDX-License-Identifier: GPL-3.0-only
//! The gated category proof against the live harness: a submitted note (the
//! Write Contract mints its stable Message-ID) categorised through the core
//! (a definition created, the note's row assigned), the mailbox-list row
//! carrying the id over a REAL read (the overlay folded onto the projection),
//! the engine proven UNTOUCHED (the note's keywords unchanged, no outbox op,
//! the note still in the INBOX unmoved), the definition deleted so the row's
//! categories empty (the cascade), and the note withdrawn so the harness keeps
//! its three-note contract. A local overlay: categories own no engine state.

use std::sync::Arc;

use crate::crypto::CryptoFlags;
use crate::{FolderRef, Intent, RowRef};

use super::{
    folder_proof::{note_ref, sender_app, withdraw_proof_folders},
    harness, inbox_key,
    proof::{self, document},
};

/// The submitted note's subject: unique among the harness fixtures.
const SUBJECT: &str = "Category proof note";

/// The words the submitted note carries.
const WORDS: &str = "Categorised by the category proof, left in place.";

/// The category round trip against the live harness: the row menu's own verb
/// over the real dial, the overlay proven on a real read, the engine untouched,
/// and the note left unmoved in the INBOX.
#[tokio::test]
async fn a_category_colours_a_note_row_and_leaves_the_engine_untouched() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, account) = sender_app(&host, &port, &user, &password).await;
    let store =
        Arc::new(astra_categories::CategoryStore::open_in_memory().expect("the category store"));
    app.install_categories_store(Arc::clone(&store));

    // The same converging sweep the folder proofs open with.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    withdraw_proof_folders(&app, &account).await;
    proof::delete_subject_mail(&app, &account, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // A submitted note carries a stable Message-ID (the seeded notes do not):
    // the Write Contract mints one before the send.
    app.dispatch(Intent::SubmitRichMail {
        from: account.clone(),
        to: "astra1@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The INBOX leads, so the rebuild the assign triggers projects the note's
    // row, and the note's keywords are captured before any category verb.
    let note = note_ref(&app, &account, SUBJECT).await;
    let inbox = FolderRef::from_parts(account.as_str(), inbox_key(&app, &account))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let keywords_before = note_keywords(&app, &account, &note).await;

    // The category: a definition created, the note's row assigned. The wire
    // carries the id; the overlay folds it onto the projected row.
    let def = app.create_category("Work", "#FF0000").expect("the create");
    app.assign_category(RowRef::Message(note.clone()), &def.id)
        .await
        .expect("the assign");
    let snapshot = app.mailbox_list();
    let row = snapshot
        .rows
        .iter()
        .find(|row| row.subject == SUBJECT)
        .expect("the note's row reads back in the INBOX");
    assert!(
        row.categories.contains(&def.id),
        "the row carries the category id over a real read"
    );

    // The engine is UNTOUCHED: the keywords are exactly as they were, no send
    // queued, and the note never left the INBOX (a category owns no engine
    // state, the local-only red line).
    let keywords_after = note_keywords(&app, &account, &note).await;
    assert_eq!(
        keywords_before, keywords_after,
        "the category verbs change no keyword"
    );
    let sending = app.sending();
    assert!(
        sending.rows.is_empty() && sending.held.is_empty(),
        "the category verbs queue no outbox op"
    );
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .any(|row| row.subject == SUBJECT),
        "the categorised note reads back in the INBOX, unmoved"
    );

    // The delete cascades the assignment off the row: the rebuild sheds the id.
    let removed = app.delete_category(&def.id).await.expect("the delete");
    assert_eq!(removed, 1, "the one assignment cascades");
    let snapshot = app.mailbox_list();
    let row = snapshot
        .rows
        .iter()
        .find(|row| row.subject == SUBJECT)
        .expect("the note's row still reads back");
    assert!(
        row.categories.is_empty(),
        "the deleted category sheds off the row"
    );

    // The withdrawal: the submitted note goes for good, the harness's
    // three-note contract intact.
    proof::delete_subject_mail(&app, &account, &[SUBJECT]).await;
    withdraw_proof_folders(&app, &account).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
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

/// The note's stored keywords, sorted so the comparison is order-free: the
/// engine's own mutable state the category verbs must never touch.
async fn note_keywords(
    app: &crate::App<Box<dyn engine_api::Provider>>,
    account: &engine_api::AccountId,
    note: &crate::MessageRef,
) -> Vec<String> {
    let mut keywords: Vec<String> = app
        .engine
        .mail_by_keys(account, std::slice::from_ref(&note.key))
        .await
        .expect("the note's row reads")
        .into_iter()
        .next()
        .expect("the note resolves")
        .keywords
        .iter()
        .map(|keyword| keyword.as_str().to_owned())
        .collect();
    keywords.sort();
    keywords
}
