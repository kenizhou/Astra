// SPDX-License-Identifier: GPL-3.0-only
//! Core category tests over the fake provider: assigning projects the id onto
//! folder, unified, and search rows in BOTH flat and threaded modes (including
//! the lone-message case and a flat message that resolves its thread id);
//! unassign removes it, delete cascades it off the rows, a rename keeps the id;
//! the REGRESSION pins that the engine is untouched (zero edit_mail, no outbox
//! op, the message stays in its inbox); and a storeless app overlays nothing.

use std::sync::{Arc, Mutex};

use astra_categories::{CategoryId, CategoryStore};
use engine_api::Message;

use crate::{
    categories::CategoryError,
    dispatch_fakes as fakes,
    protocol::Intent,
    reference::{FolderRef, MessageRef, RowRef, ThreadRef},
    ViewMode,
};

use fakes::{account, app, delivered_at, inbox, mail, threaded};

/// One app over one "work" account holding `messages`, a category store
/// installed, synced and ready. Answers the app and its store.
async fn setup(messages: Vec<Message>) -> (crate::App<fakes::FakeProvider>, Arc<CategoryStore>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], messages);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = Arc::new(CategoryStore::open_in_memory().unwrap());
    app.install_categories_store(Arc::clone(&store));
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, store)
}

/// Creates the "Work" definition and answers its id.
fn work(store: &CategoryStore) -> CategoryId {
    store.create_category("Work", "#FF0000").unwrap().id
}

/// A message row on "work" under `key`.
fn message_row(key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts("work", key.to_owned()).unwrap())
}

/// A thread row on "work" under `thread`.
fn thread_row(thread: &str) -> RowRef {
    RowRef::Thread(ThreadRef::from_parts("work", thread.to_owned()).unwrap())
}

/// Selects the work inbox in `mode`.
async fn show_inbox(app: &crate::App<fakes::FakeProvider>, mode: ViewMode) {
    app.dispatch(Intent::SetViewMode(mode)).await.unwrap();
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn assign_projects_onto_a_flat_folder_row() {
    let (app, store) = setup(vec![mail("m1", "inbox", "One")]).await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Flat).await;

    app.assign_category(message_row("m1"), &id).await.unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(fakes::row(&snapshot, "m1").categories, vec![id]);
}

#[tokio::test]
async fn assign_projects_onto_a_threaded_folder_row() {
    let (app, store) = setup(vec![
        threaded(mail("m1", "inbox", "One"), "th-1"),
        threaded(mail("m2", "inbox", "Two"), "th-1"),
    ])
    .await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Threaded).await;

    app.assign_category(thread_row("th-1"), &id).await.unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(fakes::row(&snapshot, "th-1").categories, vec![id]);
}

#[tokio::test]
async fn assign_projects_onto_a_lone_message_in_threaded_mode() {
    // A never-threaded message is its own conversation, keyed by its own
    // provider key: the assign and the overlay must agree on that key.
    let (app, store) = setup(vec![mail("m1", "inbox", "One")]).await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Threaded).await;

    app.assign_category(message_row("m1"), &id).await.unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(fakes::row(&snapshot, "m1").categories, vec![id]);
}

#[tokio::test]
async fn a_flat_message_assign_resolves_its_thread_id() {
    // A flat message row that belongs to a thread keys the assignment by the
    // resolved thread id, so the threaded view of the same conversation shows
    // it too: the flat resolution matches the projection's thread_key.
    let (app, store) = setup(vec![
        threaded(mail("m1", "inbox", "One"), "th-1"),
        threaded(mail("m2", "inbox", "Two"), "th-1"),
    ])
    .await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Flat).await;

    app.assign_category(message_row("m1"), &id).await.unwrap();
    assert_eq!(
        fakes::row(&app.mailbox_list(), "m1").categories,
        vec![id.clone()]
    );

    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();
    assert_eq!(fakes::row(&app.mailbox_list(), "th-1").categories, vec![id]);
}

#[tokio::test]
async fn assign_projects_onto_unified_rows_in_both_modes() {
    for mode in [ViewMode::Flat, ViewMode::Threaded] {
        let (app, store) = setup(vec![mail("m1", "inbox", "One")]).await;
        let id = work(&store);
        // The app opens in the unified all-inboxes view.
        app.dispatch(Intent::SetViewMode(mode)).await.unwrap();

        app.assign_category(message_row("m1"), &id).await.unwrap();

        let snapshot = app.mailbox_list();
        assert_eq!(fakes::row(&snapshot, "m1").categories, vec![id], "{mode:?}");
    }
}

#[tokio::test]
async fn assign_projects_onto_search_rows_in_both_modes() {
    for mode in [ViewMode::Flat, ViewMode::Threaded] {
        let (app, store) = setup(vec![delivered_at(mail("m1", "inbox", "monitor"), 8, 0)]).await;
        let id = work(&store);
        app.dispatch(Intent::SetViewMode(mode)).await.unwrap();
        app.dispatch(Intent::Search {
            query: Some("monitor".to_owned()),
        })
        .await
        .unwrap();

        app.assign_category(message_row("m1"), &id).await.unwrap();

        let snapshot = app.mailbox_list();
        let state = snapshot.search.as_ref().expect("the search state is live");
        assert_eq!(fakes::row_keys_from(&state.rows), ["m1"]);
        assert_eq!(state.rows[0].categories, vec![id], "{mode:?}");
    }
}

#[tokio::test]
async fn unassign_removes_the_category_from_the_row() {
    let (app, store) = setup(vec![mail("m1", "inbox", "One")]).await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Flat).await;
    app.assign_category(message_row("m1"), &id).await.unwrap();

    app.unassign_category(message_row("m1"), &id).await.unwrap();

    assert!(
        fakes::row(&app.mailbox_list(), "m1").categories.is_empty(),
        "the row carries no category"
    );
}

#[tokio::test]
async fn delete_cascades_the_category_off_the_rows() {
    let (app, store) = setup(vec![mail("m1", "inbox", "One")]).await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Flat).await;
    app.assign_category(message_row("m1"), &id).await.unwrap();

    let removed = app.delete_category(&id).await.unwrap();

    assert_eq!(removed, 1);
    assert!(
        fakes::row(&app.mailbox_list(), "m1").categories.is_empty(),
        "the row carries no category"
    );
}

#[tokio::test]
async fn a_rename_keeps_the_id_on_the_row() {
    let (app, store) = setup(vec![mail("m1", "inbox", "One")]).await;
    let id = work(&store);
    show_inbox(&app, ViewMode::Flat).await;
    app.assign_category(message_row("m1"), &id).await.unwrap();

    app.update_category(&id, "Projects", "#00FF00").unwrap();

    // The wire carries ids only, so the row keeps the same id through a rename.
    assert_eq!(fakes::row(&app.mailbox_list(), "m1").categories, vec![id]);
}

#[tokio::test]
async fn the_engine_is_untouched_by_every_category_verb() {
    // The local-only red line: no edit_mail, no outbox op, no folder edit, and
    // the message stays in its inbox.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![mail("m1", "inbox", "One")]);
    let edits = Arc::clone(&provider.edits);
    let submits = Arc::clone(&provider.submits);
    let mailbox_edits = Arc::clone(&provider.mailbox_edits);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = Arc::new(CategoryStore::open_in_memory().unwrap());
    app.install_categories_store(Arc::clone(&store));
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let id = work(&store);
    show_inbox(&app, ViewMode::Flat).await;

    app.assign_category(message_row("m1"), &id).await.unwrap();
    app.unassign_category(message_row("m1"), &id).await.unwrap();
    app.assign_category(message_row("m1"), &id).await.unwrap();
    app.delete_category(&id).await.unwrap();

    assert!(edits.lock().unwrap().is_empty(), "no edit_mail queued");
    assert!(submits.lock().unwrap().is_empty(), "no outbox op queued");
    assert!(
        mailbox_edits.lock().unwrap().is_empty(),
        "no folder edit queued"
    );
    let stored = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&stored),
        ["m1"],
        "the message stays in its inbox"
    );
}

#[tokio::test]
async fn a_storeless_app_overlays_nothing_and_lists_empty() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![mail("m1", "inbox", "One")]);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    show_inbox(&app, ViewMode::Flat).await;

    assert!(
        fakes::row(&app.mailbox_list(), "m1").categories.is_empty(),
        "the row carries no category"
    );
    assert!(
        app.list_categories().unwrap().is_empty(),
        "no store, no definitions"
    );
}

#[tokio::test]
async fn a_storeless_write_refuses() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![mail("m1", "inbox", "One")]);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let id = CategoryId::new("work".to_owned());

    assert!(matches!(
        app.assign_category(message_row("m1"), &id).await,
        Err(CategoryError::NoStore)
    ));
    assert!(matches!(
        app.create_category("Work", "#FF0000"),
        Err(CategoryError::NoStore)
    ));
}

#[tokio::test]
async fn assigning_an_unknown_category_refuses() {
    let (app, _store) = setup(vec![mail("m1", "inbox", "One")]).await;
    show_inbox(&app, ViewMode::Flat).await;
    let ghost = CategoryId::new("ghost".to_owned());

    let outcome = app.assign_category(message_row("m1"), &ghost).await;

    assert!(matches!(
        outcome,
        Err(CategoryError::Store(
            astra_categories::CategoryError::UnknownCategory(_)
        ))
    ));
}
