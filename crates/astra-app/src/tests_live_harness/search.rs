// SPDX-License-Identifier: GPL-3.0-only
//! The gated search test against the live harness: real queries over the
//! corpus the seed already pinned, through the engine's real search surface,
//! read back through the product's own dial, sync, and dispatch. No seed of
//! its own and no cleanup: a search only reads, so the harness keeps its
//! seeded shape whatever this suite finds.
//!
//! Each assertion is a contract the search service owns: a subject hit is one
//! seeded note exactly (the engine's full-text match over subject text), a
//! `from:` filter hit crosses into the engine's own DSL and reaches every
//! seeded note across two folders (the three INBOX notes and the rich note in
//! Projects), a no-hit query is an honest empty answer that still states how
//! far back it looked, and clearing the search returns the folder view it was
//! opened from.

use std::sync::Arc;

use astra_viewmodel::SearchScope;
use engine_api::{EmailAddress, Engine};

use crate::{Account, App, AppObserver, FolderRef, Intent};

use super::{dial_with_accepted_certificate, harness, inbox_key, CountingObserver, NOTES};

/// The search surface against the live harness: the subject hit, the `from:`
/// filter hit, and the no-hit state, each through the real engine, and the
/// clear that returns the folder view the search displaced.
///
/// Like the list test, the run builds the loop the desktop host builds: the
/// engine in a scratch store, the providers the accepted dial bound, the app
/// over them, `dispatch` awaiting each whole pass so the snapshot is final
/// when it returns. The queries pick words the corpus holds exactly once:
/// "three" sits in one subject and one subject only (the sender's display
/// name holds "two", so the count words would not separate the notes), and
/// every seeded note, the rich note included, carries "harness" in its
/// subject from the one sender the filter names.
#[tokio::test]
async fn search_hits_the_seeded_notes_through_the_real_engine() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account_id, providers) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;

    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers,
            identity: EmailAddress::new("astra1@example.test"),
        }],
        None,
        observer,
    );

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let inbox = FolderRef::from_parts(account_id.as_str(), inbox_key(&app, &account_id))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();

    // The subject hit: exactly the one seeded note whose subject holds the
    // words, newest first trivially, the folder rows stood down while the
    // search state is live.
    app.dispatch(Intent::Search {
        query: Some("note three".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.is_empty(),
        "the folder rows stand down while the search is live"
    );
    let state = snapshot
        .search
        .as_ref()
        .expect("the search state publishes");
    assert_eq!(state.query, "note three", "the echo shows the raw query");
    assert_eq!(
        state.scope,
        SearchScope::AllExceptTrash,
        "the default scope reaches everywhere but the trash"
    );
    assert_eq!(
        state.rows.len(),
        1,
        "one seeded note holds both words: {:?}",
        state
            .rows
            .iter()
            .map(|row| row.subject.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(state.rows[0].subject, "Harness note three");
    assert_eq!(state.rows[0].sender_address, "astra2@example.test");
    assert_eq!(state.depth_months, 3, "the synced window is three months");

    // The from: filter hit: the split reaches the engine as a structured
    // address filter beside the text, and every seeded note matches, the rich
    // note included from Projects, newest first.
    app.dispatch(Intent::Search {
        query: Some("from:astra2@example.test harness".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot
        .search
        .as_ref()
        .expect("the search state publishes");
    let subjects: Vec<&str> = state.rows.iter().map(|row| row.subject.as_str()).collect();
    assert_eq!(
        subjects,
        [
            "Harness rich note",
            "Harness note three",
            "Harness note two",
            "Harness note one",
        ],
        "every seeded note across two folders, newest first"
    );

    // The no-hit state: an honest empty answer, the query echoed, the horizon
    // still stated (the empty answer claims only as far back as it looked).
    app.dispatch(Intent::Search {
        query: Some("zebra".to_owned()),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let state = snapshot
        .search
        .as_ref()
        .expect("the search state publishes");
    assert_eq!(state.query, "zebra");
    assert!(
        state.rows.is_empty(),
        "no seeded note matches, and nothing leaks in"
    );
    assert_eq!(
        state.depth_months, 3,
        "the no-hit answer still says how far back it looked"
    );
    assert!(
        snapshot.rows.is_empty(),
        "no folder row leaks through a no-hit search"
    );

    // Leaving search restores the view it was opened from: the INBOX's three
    // notes, unsearched.
    app.dispatch(Intent::Search { query: None }).await.unwrap();
    let snapshot = app.mailbox_list();
    assert_eq!(snapshot.search, None, "the clear drops the search state");
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    let expected: Vec<&str> = NOTES.iter().rev().copied().collect();
    assert_eq!(subjects, expected, "the INBOX rows return, newest first");
}
