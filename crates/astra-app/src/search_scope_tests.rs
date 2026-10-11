// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the search scope: which accounts and folders each scope variant
//! covers, resolved against the live selection the mailbox list is showing,
//! the thread grouping the results follow, and the depth disclosure the state
//! carries. The fixtures live in `dispatch_fakes`; the DSL and publish tests
//! live in `search_tests`.

use std::sync::{Arc, Mutex};

use engine_api::Mailbox;

use crate::{
    dispatch_fakes as fakes,
    protocol::{Intent, SearchScope, Surface},
    reference::FolderRef,
    ViewMode,
};

use fakes::{account, app, delivered_at, folder, inbox, mail, threaded, trash};

/// The tree every scope fixture walks: an Inbox, an Archive, and a Trash.
fn tree() -> Vec<Mailbox> {
    vec![inbox(), folder("archive", "Archive"), trash()]
}

/// One app over one account holding `messages`, synced and ready to search.
async fn searched_app(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    messages: Vec<engine_api::Message>,
) -> crate::App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with_mail(tree(), messages);
    let app = app(vec![account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// How many times the mailbox-list surface has been signalled.
fn list_signals(surfaces: &Arc<Mutex<Vec<Surface>>>) -> usize {
    surfaces
        .lock()
        .unwrap()
        .iter()
        .filter(|surface| **surface == Surface::MailboxList)
        .count()
}

#[tokio::test]
async fn the_default_scope_excludes_trash_and_narrowing_to_the_folder_finds_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("m2", "trash", "monitor the deleted one"), 9, 0),
        ],
    )
    .await;

    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["m1"],
        "the default scope searched everywhere but the trash"
    );

    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "trash".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["m2"],
        "a folder you cannot search is a folder you cannot use: trash stays reachable"
    );
}

#[tokio::test]
async fn the_current_folder_scope_mirrors_the_live_selection() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("m2", "archive", "monitor the filed one"), 9, 0),
        ],
    )
    .await;

    // The unified view: "this folder" is the inboxes on screen, nothing else.
    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(fakes::row_keys_from(&state.rows), ["m1"]);

    // What is on screen answers the query as it now stands: selecting another
    // folder re-runs the live search against it, no new dispatch needed.
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "archive".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(fakes::row_keys_from(&state.rows), ["m2"]);
    assert_eq!(state.scope, SearchScope::CurrentFolder);
}

#[tokio::test]
async fn the_current_folder_scope_in_an_account_scope_converges_with_current_account() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("m2", "archive", "monitor the filed one"), 9, 0),
            delivered_at(mail("m3", "trash", "monitor the deleted one"), 7, 0),
        ],
    )
    .await;
    app.dispatch(Intent::SelectAccount(Some(
        engine_api::AccountId::try_from("work").unwrap(),
    )))
    .await
    .unwrap();

    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let whole_account = snapshot.search.as_ref().unwrap().clone();
    assert_eq!(
        fakes::row_keys_from(&whole_account.rows),
        ["m2", "m1"],
        "the default scope still excludes the trash"
    );

    // The account's own all-mail view is showing, so "the current folder" is
    // the whole mailbox: the same answer the account scope gives.
    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let folder_reading = snapshot.search.as_ref().unwrap().clone();
    assert_eq!(
        fakes::row_keys_from(&folder_reading.rows),
        ["m2", "m1", "m3"]
    );

    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentAccount,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let account_reading = snapshot.search.as_ref().unwrap().clone();
    assert_eq!(
        folder_reading.rows, account_reading.rows,
        "the two readings of the same live selection converge"
    );
    assert_eq!(account_reading.scope, SearchScope::CurrentAccount);
}

#[tokio::test]
async fn set_search_scope_re_runs_the_live_query_and_is_a_noop_without_one() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("m2", "archive", "monitor the filed one"), 9, 0),
        ],
    )
    .await;

    // No query is live: nothing to re-run, and a narrowing the user cannot
    // see is one they will not think of, so nothing is stored either.
    let before = list_signals(&surfaces);
    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    assert_eq!(list_signals(&surfaces), before, "nothing was re-run");
    assert_eq!(app.mailbox_list().search, None);

    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        state.scope,
        SearchScope::AllExceptTrash,
        "the ignored narrowing stored nothing"
    );
    assert_eq!(fakes::row_keys_from(&state.rows), ["m2", "m1"]);

    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(state.scope, SearchScope::CurrentFolder, "the scope echoed");
    assert_eq!(fakes::row_keys_from(&state.rows), ["m1"]);
}

#[tokio::test]
async fn threaded_results_carry_whole_threads_on_the_newest_match() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            threaded(
                delivered_at(mail("m1", "inbox", "monitor the merger"), 8, 0),
                "t1",
            ),
            threaded(
                delivered_at(mail("m2", "inbox", "re: lunch on thursday"), 9, 0),
                "t1",
            ),
            threaded(
                delivered_at(mail("m3", "inbox", "monitor the roof"), 8, 30),
                "t2",
            ),
        ],
    )
    .await;
    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();
    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();

    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["m3", "t1"],
        "the newest match first: t2 at 8:30, then t1 at 8:00"
    );
    let lone = &state.rows[0];
    assert_eq!(
        lone.thread_count, None,
        "a match that is a conversation of one stays a message row"
    );
    let conversation = &state.rows[1];
    assert_eq!(
        conversation.subject, "monitor the merger",
        "labelled by the message that matched, not the newer one that did not"
    );
    assert_eq!(
        conversation.thread_count,
        Some(2),
        "carrying the whole thread"
    );
}

#[tokio::test]
async fn results_merge_newest_first_across_accounts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let work = fakes::FakeProvider::with_mail(
        tree(),
        vec![
            delivered_at(mail("w1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("w2", "inbox", "monitor the same instant"), 9, 0),
        ],
    );
    let home = fakes::FakeProvider::with_mail(
        tree(),
        vec![delivered_at(
            mail("h1", "inbox", "monitor the evening"),
            9,
            0,
        )],
    );
    let app = app(
        vec![account("work", work), account("home", home)],
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["h1", "w2", "w1"],
        "the accounts merge newest first, ties broken on the account"
    );
    assert_eq!(
        state.depth_months, 3,
        "the narrowest sync depth among the searched accounts"
    );
}

#[tokio::test]
async fn a_scope_that_searches_no_account_states_a_zero_depth() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![folder("all", "All mail")],
        vec![delivered_at(mail("m1", "all", "monitor the launch"), 8, 0)],
    );
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The account has no Inbox, so "the current folder" in the unified view
    // searches nothing: no rows, and no corpus to describe.
    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(state.depth_months, 3);

    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert!(state.rows.is_empty());
    assert_eq!(
        state.depth_months, 0,
        "no account was searched, so the horizon claims nothing"
    );
}
