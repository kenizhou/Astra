// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the search service's query half: the DSL split, the run over the
//! engine's search surface, and the state a search publishes and clears onto
//! the mailbox-list snapshot. The scope, grouping, and depth tests live in
//! `search_scope_tests`; the fixtures live in `dispatch_fakes`, the crate-root
//! test module every dispatch-shaped test shares.

use std::sync::{Arc, Mutex};

use engine_api::{EmailAddress, Mailbox, Message};

use crate::{
    dispatch_fakes as fakes,
    protocol::{Intent, SearchScope, Surface},
    reference::FolderRef,
    search::split_query,
};

use fakes::{account, app, delivered_at, folder, inbox, mail, trash};

/// Rewrites who `message` is from, so a `from:` filter has two senders to
/// separate.
fn sender(mut message: Message, name: &str, address: &str) -> Message {
    message.envelope.from = vec![EmailAddress::named(name, address)];
    message
}

/// Rewrites who `message` is addressed to, so a `to:` filter has two
/// recipients to separate.
fn recipient(mut message: Message, address: &str) -> Message {
    message.envelope.to = vec![EmailAddress::new(address.to_owned())];
    message
}

/// The tree every scope fixture walks: an Inbox, an Archive, and a Trash.
fn tree() -> Vec<Mailbox> {
    vec![inbox(), folder("archive", "Archive"), trash()]
}

/// One app over one account holding `messages`, synced and ready to search.
async fn searched_app(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    messages: Vec<Message>,
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

#[test]
fn split_query_extracts_the_prefix_filters_and_keeps_the_remainder() {
    let cases: &[(&str, Option<&str>, Option<&str>, &str)] = &[
        ("monitor report", None, None, "monitor report"),
        (
            "from:ada@example.test monitor",
            Some("ada@example.test"),
            None,
            "monitor",
        ),
        (
            "monitor to:grace@example.test report",
            None,
            Some("grace@example.test"),
            "monitor report",
        ),
        (
            "FROM:Ada@Example.Test TO:Grace@Example.Test monitor",
            Some("Ada@Example.Test"),
            Some("Grace@Example.Test"),
            "monitor",
        ),
        // The operator keyword is case-blind; the value crosses verbatim, the
        // same rule the engine's own parser keeps.
        (
            "from:a@example.test from:b@example.test monitor",
            Some("b@example.test"),
            None,
            "monitor",
        ),
    ];
    for (raw, from, to, text) in cases {
        let split = split_query(raw);
        assert_eq!(split.0.as_deref(), *from, "the from filter of {raw:?}");
        assert_eq!(split.1.as_deref(), *to, "the to filter of {raw:?}");
        assert_eq!(split.2, *text, "the free text of {raw:?}");
    }
}

#[test]
fn split_query_tolerates_malformed_prefixes() {
    // An unknown prefix is free text, never an error and never a filter.
    let unknown = split_query("fromm:x monitor");
    assert_eq!(unknown.0, None);
    assert_eq!(unknown.2, "fromm:x monitor");
    // A prefix with no value names nobody: dropped rather than passed on, so
    // the engine is never handed a query it would refuse outright.
    let empty_from = split_query("from: monitor");
    assert_eq!(empty_from.0, None, "from: with no value is not a filter");
    assert_eq!(empty_from.2, "monitor", "and it is not text either");
    let empty_to = split_query("to: monitor");
    assert_eq!(empty_to.1, None);
    assert_eq!(empty_to.2, "monitor");
    // The engine's own operators are not this split's to take: they stay in
    // the remainder for the engine to parse.
    let passthrough = split_query("subject:lunch from:ada@example.test");
    assert_eq!(passthrough.0.as_deref(), Some("ada@example.test"));
    assert_eq!(passthrough.2, "subject:lunch");
}

#[test]
fn split_query_binds_quoted_values_and_phrases() {
    let quoted_value = split_query("from:\"ada lovelace\" monitor");
    assert_eq!(quoted_value.0.as_deref(), Some("ada lovelace"));
    assert_eq!(quoted_value.2, "monitor");
    // A quoted phrase stays one token of the remainder, quotes and all, so
    // the engine reads back the phrase the user typed.
    let phrase = split_query("\"q report\" monitor");
    assert_eq!(phrase.2, "\"q report\" monitor");
    let empty_quotes = split_query("from:\"\" monitor");
    assert_eq!(empty_quotes.0, None, "empty quotes name nobody");
    assert_eq!(empty_quotes.2, "monitor");
}

#[tokio::test]
async fn search_publishes_the_state_and_suppresses_the_folder_rows() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("m2", "inbox", "lunch on thursday"), 9, 0),
            delivered_at(mail("m3", "archive", "monitor the old one"), 7, 0),
        ],
    )
    .await;
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(fakes::row_keys(&app.mailbox_list()), ["m2", "m1"]);
    assert_eq!(app.mailbox_list().search, None, "no search is live yet");

    app.dispatch(Intent::Search {
        query: Some("from:ada@example.test monitor".to_owned()),
    })
    .await
    .unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(
        snapshot.rows,
        Vec::new(),
        "the folder rows stand down while the search state is live"
    );
    let state = snapshot
        .search
        .as_ref()
        .expect("the search state publishes");
    assert_eq!(state.query, "from:ada@example.test monitor");
    assert_eq!(state.scope, SearchScope::AllExceptTrash);
    assert_eq!(fakes::row_keys_from(&state.rows), ["m1", "m3"]);
    assert_eq!(state.rows[0].account_id.as_str(), "work");
    assert_eq!(state.rows[0].subject, "monitor the launch");
    assert_eq!(state.depth_months, 3, "the synced window is three months");
    assert!(list_signals(&surfaces) >= 2, "the list surface signalled");
}

#[tokio::test]
async fn clearing_returns_the_folder_rows_and_resets_the_scope() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "monitor the launch"), 8, 0),
            delivered_at(mail("m2", "inbox", "lunch on thursday"), 9, 0),
            delivered_at(mail("m3", "archive", "monitor the filed one"), 7, 30),
            delivered_at(mail("m4", "trash", "monitor the deleted one"), 7, 0),
        ],
    )
    .await;
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
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
    assert!(app.mailbox_list().search.is_some());

    // A blank field clears just as an explicit None does: empty is
    // navigation, not a search for nothing.
    app.dispatch(Intent::Search {
        query: Some("   ".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    assert_eq!(snapshot.search, None, "the blank query cleared the search");
    assert_eq!(fakes::row_keys(&snapshot), ["m2", "m1"]);

    // The narrowing went with it: the next search opens across everything.
    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(state.scope, SearchScope::AllExceptTrash);
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["m1", "m3"],
        "the reset scope reached past the inbox, still short of the trash"
    );

    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    app.dispatch(Intent::Search { query: None }).await.unwrap();
    assert_eq!(
        fakes::row_keys(&app.mailbox_list()),
        ["m2", "m1"],
        "an explicit None returns the folder rows the same way"
    );
}

#[tokio::test]
async fn the_prefix_filters_reach_the_engine() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![
            delivered_at(
                recipient(
                    sender(
                        mail("m1", "inbox", "monitor the launch"),
                        "Ada",
                        "ada@example.test",
                    ),
                    "grace@example.test",
                ),
                8,
                0,
            ),
            delivered_at(
                recipient(
                    sender(
                        mail("m2", "inbox", "monitor the launch"),
                        "Grace",
                        "grace@example.test",
                    ),
                    "ada@example.test",
                ),
                9,
                0,
            ),
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
    assert_eq!(fakes::row_keys_from(&state.rows), ["m2", "m1"]);

    app.dispatch(Intent::Search {
        query: Some("from:ada@example.test monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["m1"],
        "the from filter matched the sender, case and exact"
    );

    app.dispatch(Intent::Search {
        query: Some("to:grace@example.test monitor".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().unwrap();
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        ["m1"],
        "the to filter matched the recipient the same way"
    );
}

#[tokio::test]
async fn a_prefix_with_no_value_searches_nothing_rather_than_everything() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = searched_app(
        &surfaces,
        vec![delivered_at(
            mail("m1", "inbox", "monitor the launch"),
            8,
            0,
        )],
    )
    .await;

    app.dispatch(Intent::Search {
        query: Some("from:".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot.search.as_ref().expect("the search is live");
    assert_eq!(state.query, "from:", "the echo shows what was typed");
    assert_eq!(
        fakes::row_keys_from(&state.rows),
        Vec::<String>::new(),
        "a filter that names nobody matches nobody"
    );
}
