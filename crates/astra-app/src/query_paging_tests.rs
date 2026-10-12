// SPDX-License-Identifier: GPL-3.0-only
//! The scope table's middle row, search paging, and the windowed flag:
//! split from the query tests for the 500-line cap, same fixtures.

use super::*;

#[tokio::test]
async fn a_search_narrowed_to_the_account_alone_spans_the_whole_mailbox() {
    // The scope table's middle row: an account named with no folder searches
    // that account's everything, Trash included, beside the default scope
    // that skips it.
    let provider = fakes::FakeProvider::with_mail(
        tree(),
        vec![
            mail("m1", "inbox", "Invoice for March"),
            mail("m2", "trash", "Invoice for February"),
        ],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    let whole = app
        .query_search("Invoice", Some(&account_id), None, 0, 20)
        .await;
    assert_eq!(
        subjects(&whole),
        ["Invoice for March", "Invoice for February"],
        "the account's own scope spans the trash beside the inbox",
    );
}

#[tokio::test]
async fn a_search_pages_its_hits_like_a_listing_does() {
    let provider = fakes::FakeProvider::with_mail(
        tree(),
        vec![
            mail("m1", "inbox", "Invoice one"),
            mail("m2", "inbox", "Invoice two"),
            mail("m3", "inbox", "Invoice three"),
        ],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let first = app.query_search("Invoice", None, None, 0, 2).await;
    // The fixture shares one timestamp, so the tiebreak (the key order)
    // decides: m1 before m2 before m3, cut by the page.
    assert_eq!(subjects(&first), ["Invoice one", "Invoice two"]);
    assert_eq!(first.total, 3, "the total spans the pages");
    let second = app.query_search("Invoice", None, None, 2, 2).await;
    assert_eq!(subjects(&second), ["Invoice three"]);
    assert_eq!(second.offset, 2, "the offset echoes back");
}

#[tokio::test]
async fn a_small_folder_answers_unwindowed_and_a_full_window_says_so() {
    // The `windowed` flag exists so "older mail exists that paging cannot
    // reach" stays tellable apart from "the folder is empty": a folder
    // inside the store's window answers false, and a listing cut by the
    // window itself answers true.
    let provider = fakes::FakeProvider::with_mail(tree(), vec![mail("m1", "inbox", "Only one")]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let holder = app(vec![account("acct-1", provider)], &surfaces);
    holder.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    let small = holder
        .query_messages(&account_id, Some("inbox"), false, 0, 20)
        .await;
    assert!(!small.windowed, "a folder inside the window is not a cut");
    assert_eq!(small.total, 1);

    // A store holding a full window of mail: the engine's window is the
    // cut, and the page says so whatever the page's own size.
    let many: Vec<_> = (0..crate::window::MIN_LOAD_WINDOW + 10)
        .map(|at| mail(&format!("m{at}"), "inbox", "Bulk"))
        .collect();
    let provider = fakes::FakeProvider::with_mail(tree(), many);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let holder = app(vec![account("acct-1", provider)], &surfaces);
    holder.dispatch(Intent::RefreshMail).await.unwrap();
    let cut = holder
        .query_messages(&account_id, Some("inbox"), false, 0, 20)
        .await;
    assert!(
        cut.windowed,
        "a listing cut by the store's own window says older mail exists"
    );
}
