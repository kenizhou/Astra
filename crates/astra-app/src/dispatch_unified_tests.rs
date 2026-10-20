// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the unified All Accounts group: the scope's rows merge every
//! account's inbox newest first, the same row shape the per-folder list
//! renders, and the chosen scope persists across launches. The fixtures live
//! in `dispatch_fakes`; this module hangs off the dispatch tests beside the
//! folder-scoped list tests.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::{dispatch_fakes as fakes, reference::MessageRef, Intent, Scope, Surface};

use fakes::{account, app, app_with_prefs, delivered_at, folder, inbox, mail, scratch, threaded};

/// The unified scope's account pair: `work` and `home`, each an Inbox and an
/// Archive, so the merged list has two inboxes to draw from.
async fn two_inboxes(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    work_mail: Vec<engine_api::Message>,
    home_mail: Vec<engine_api::Message>,
) -> crate::App<fakes::FakeProvider> {
    let tree = || vec![inbox(), folder("archive", "Archive")];
    let app = app(
        vec![
            account("work", fakes::FakeProvider::with_mail(tree(), work_mail)),
            account("home", fakes::FakeProvider::with_mail(tree(), home_mail)),
        ],
        surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

#[tokio::test]
async fn the_unified_view_lists_every_accounts_inbox_rows_newest_first() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = two_inboxes(
        &surfaces,
        vec![
            delivered_at(mail("w1", "inbox", "Work one"), 8, 0),
            delivered_at(mail("w2", "inbox", "Work two"), 9, 0),
            delivered_at(mail("w-filed", "archive", "Work filed"), 10, 0),
        ],
        vec![
            delivered_at(mail("h1", "inbox", "Home one"), 8, 30),
            delivered_at(mail("h2", "inbox", "Home two"), 7, 0),
        ],
    )
    .await;

    let snapshot = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&snapshot),
        ["w2", "h1", "w1", "h2"],
        "every account's inbox mail, merged newest first"
    );
    assert_eq!(
        fakes::row(&snapshot, "w2").account_id.as_str(),
        "work",
        "a row carries its own account"
    );
    assert_eq!(fakes::row(&snapshot, "h1").account_id.as_str(), "home");
}

#[tokio::test]
async fn the_unified_view_groups_threads_within_their_own_account() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = two_inboxes(
        &surfaces,
        vec![
            threaded(delivered_at(mail("w1", "inbox", "Re: Plan"), 8, 0), "th-1"),
            threaded(delivered_at(mail("w2", "inbox", "Re: Plan"), 8, 10), "th-1"),
        ],
        vec![threaded(
            delivered_at(mail("h1", "inbox", "Re: Plan"), 8, 20),
            "th-1",
        )],
    )
    .await;
    app.dispatch(Intent::SetViewMode(crate::ViewMode::Threaded))
        .await
        .unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&snapshot),
        ["h1", "th-1"],
        "two conversations: a thread id belongs to one account, never both, and a \
         one-member conversation is its own message row"
    );
    let accounts: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.account_id.as_str())
        .collect();
    assert_eq!(
        accounts,
        ["home", "work"],
        "the home conversation's newest member is the later one, so it leads"
    );
    assert_eq!(
        snapshot.rows[1].thread_count,
        Some(2),
        "the work conversation counts its own two members"
    );
}

#[tokio::test]
async fn a_row_acted_on_in_the_unified_view_writes_through_its_own_account() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = two_inboxes(
        &surfaces,
        vec![delivered_at(mail("w1", "inbox", "Work one"), 8, 0)],
        vec![delivered_at(mail("h1", "inbox", "Home one"), 9, 0)],
    )
    .await;

    app.dispatch(Intent::MarkRead {
        row: crate::RowRef::Message(MessageRef::from_parts("home", "h1".to_owned()).unwrap()),
        read: true,
    })
    .await
    .unwrap();

    let snapshot = app.mailbox_list();
    assert!(
        !fakes::row(&snapshot, "h1").unread,
        "the home row flipped through its own account"
    );
    assert!(
        fakes::row(&snapshot, "w1").unread,
        "the work row did not move"
    );
}

#[tokio::test]
async fn the_selected_scope_persists_across_launches() {
    let dir = scratch("unified-scope");
    let prefs_path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let tree = || vec![inbox(), folder("archive", "Archive")];
    let build = || {
        app_with_prefs(
            vec![
                account("work", fakes::FakeProvider::with(tree())),
                account("home", fakes::FakeProvider::with(tree())),
            ],
            Some(prefs_path.clone()),
            &surfaces,
        )
    };
    let app = build();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The account view: a relaunch reopens it, not the unified default.
    app.dispatch(Intent::SelectAccount(Some(
        AccountId::try_from("home").unwrap(),
    )))
    .await
    .unwrap();
    let relaunched = build();
    assert_eq!(
        relaunched.scope_for_test(),
        Scope::Account(AccountId::try_from("home").unwrap()),
        "an account selection survives a relaunch"
    );

    // The unified view: choosing it after another scope persists that choice.
    relaunched
        .dispatch(Intent::SelectAccount(None))
        .await
        .unwrap();
    let unified = build();
    assert_eq!(
        unified.scope_for_test(),
        Scope::AllInboxes,
        "the unified choice survives a relaunch"
    );
    assert!(
        crate::load_preferences(&prefs_path)
            .selected_folder
            .is_none(),
        "no folder stays selected beside the unified scope"
    );
}
