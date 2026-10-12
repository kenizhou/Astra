// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the query layer: the two guarantee tests that pin the two doors
//! (a read republishes nothing, marks nothing, moves nothing), the contrast
//! beside the first that states why the layer exists, and the paging, scope,
//! and body contracts the read tools answer under. The fixtures live in
//! `dispatch_fakes`, the crate-root test module every dispatch-shaped test
//! shares.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, Mailbox};
use engine_provider::MailEdit;

use crate::{
    dispatch_fakes as fakes,
    query::{MessagePage, MAX_PAGE},
    reference::MessageRef,
    Intent, SearchScope,
};

use fakes::{account, app, delivered_at, flagged, folder, inbox, mail, read, trash};

/// The reference to message `key` of `account`.
fn msg(account: &str, key: &str) -> MessageRef {
    MessageRef::from_parts(account, key.to_owned()).unwrap()
}

/// The tree every fixture walks: a role-bearing Inbox, a plain Archive, and a
/// role-bearing Trash, so folder filters and the trash scope rule have all
/// three shapes to read.
fn tree() -> Vec<Mailbox> {
    vec![inbox(), folder("archive", "Archive"), trash()]
}

/// A minimal RFC 5322 source whose body is one `text/plain` part holding
/// `text`, so the body read exercises the plain-by-construction path with no
/// HTML anywhere in the message.
fn plain_source(text: &str) -> Vec<u8> {
    format!("Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n{text}").into_bytes()
}

/// Every read the query layer answers, run against one fixture.
async fn run_every_read(app: &crate::App<fakes::FakeProvider>, account_id: &AccountId) {
    app.query_accounts().await;
    app.query_folders(account_id).await;
    app.query_messages(account_id, Some("inbox"), false, 0, 20)
        .await;
    app.query_messages(account_id, None, true, 0, 20).await;
    app.query_search("report", None, None, 0, 20).await;
    app.query_search("report", Some(account_id), Some("inbox"), 0, 20)
        .await;
    app.query_message(&msg(account_id.as_str(), "m1")).await;
}

#[tokio::test]
async fn reads_never_mutate_the_server() {
    // GUARANTEE 1. `Intent::OpenMessage` marks a message read ON THE SERVER,
    // so an assistant answering "read me that email" over the intent path
    // would silently clear the unread badge on the user's real mailbox: an
    // irreversible side effect of a question. Every read here must leave the
    // provider untouched.
    let provider = fakes::FakeProvider::with_sources(
        tree(),
        vec![mail("m1", "inbox", "Quarterly report")],
        vec![("m1".to_owned(), plain_source("The numbers are in."))],
    );
    let edits = Arc::clone(&provider.edits);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    run_every_read(&app, &account_id).await;

    assert!(
        edits.lock().unwrap().is_empty(),
        "no read reached the provider as an edit: {:?}",
        edits.lock().unwrap(),
    );
    assert!(
        app.query_message(&msg("acct-1", "m1"))
            .await
            .expect("the message resolves")
            .unread,
        "and the message is still unread after being read in full",
    );
}

#[tokio::test]
async fn the_intent_path_by_contrast_does_mark_it_read() {
    // The paired contrast, in the same file on purpose: this states WHY
    // `query_message` exists. If this ever stops recording a `$seen` write,
    // `reads_never_mutate_the_server` has become a test of nothing: a check
    // that cannot fail is not a check.
    let provider = fakes::FakeProvider::with_sources(
        tree(),
        vec![mail("m1", "inbox", "Quarterly report")],
        vec![("m1".to_owned(), plain_source("The numbers are in."))],
    );
    let edits = Arc::clone(&provider.edits);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::OpenMessage {
        message: msg("acct-1", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        *edits.lock().unwrap(),
        vec![MailEdit::mark_seen(
            engine_api::ProviderKey::new("m1").unwrap(),
            true,
        )],
        "opening it wrote exactly the one edit that marks it seen",
    );
}

#[tokio::test]
async fn reads_never_move_the_users_screen() {
    // GUARANTEE 2. Every read-shaped Intent ends in a snapshot rebuild, and
    // Search also rewrites the live query and scope. An assistant listing a
    // folder or running a search must not scroll, re-scope or re-filter the
    // list of a person who is reading something else.
    let provider = fakes::FakeProvider::with_mail(
        tree(),
        vec![
            mail("m1", "inbox", "Quarterly report"),
            mail("m2", "inbox", "Lunch plans"),
        ],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // Put the user somewhere specific and narrow: one folder, an active
    // search that has been scoped down, and a window they have scrolled.
    app.dispatch(Intent::SelectFolder {
        folder: crate::FolderRef::from_parts("acct-1", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::Search {
        query: Some("lunch".to_owned()),
    })
    .await
    .unwrap();
    app.dispatch(Intent::SetSearchScope {
        scope: SearchScope::CurrentFolder,
    })
    .await
    .unwrap();
    app.dispatch(Intent::ShowMore).await.unwrap();

    let before = (
        app.mailbox_list(),
        app.scope_for_test(),
        app.search_session(),
        app.visible_limit(),
    );
    surfaces.lock().unwrap().clear();

    let account_id = AccountId::try_from("acct-1").unwrap();
    run_every_read(&app, &account_id).await;

    let after = (
        app.mailbox_list(),
        app.scope_for_test(),
        app.search_session(),
        app.visible_limit(),
    );
    assert_eq!(
        before.0, after.0,
        "the published mailbox list is byte-identical",
    );
    assert_eq!(
        before.1, after.1,
        "the account and folder on screen did not move",
    );
    assert_eq!(before.2, after.2, "the user's search query survived");
    assert_eq!(
        before.3, after.3,
        "and so did the scope they narrowed to and the window they scrolled",
    );
    assert!(
        surfaces.lock().unwrap().is_empty(),
        "no surface was signalled at all: the host was never told to re-pull: {:?}",
        surfaces.lock().unwrap(),
    );
}

#[tokio::test]
async fn a_read_does_not_warm_the_uis_row_caches() {
    // GUARANTEE 3. The UI projects from two in-memory caches; a query paging
    // deep must not leave either one holding thousands of rows the user never
    // asked to see. The query reads the store directly for exactly this
    // reason.
    let provider = fakes::FakeProvider::with_mail(tree(), vec![mail("m1", "inbox", "Hello")]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.invalidate_list_cache();

    let account_id = AccountId::try_from("acct-1").unwrap();
    run_every_read(&app, &account_id).await;

    assert!(
        !app.list_cache_is_loaded(),
        "the query left the tree cache exactly as it found it: dropped, not repopulated",
    );
    assert!(
        !app.mail_row_cache_is_loaded(),
        "and the folder row cache too",
    );
}

#[tokio::test]
async fn a_folder_page_is_newest_first_and_pages_without_gaps_or_repeats() {
    let provider = fakes::FakeProvider::with_mail(
        tree(),
        vec![
            delivered_at(mail("m1", "inbox", "Oldest"), 9, 0),
            delivered_at(mail("m2", "inbox", "Middle"), 10, 0),
            delivered_at(mail("m3", "inbox", "Newest"), 11, 0),
        ],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    let first = app
        .query_messages(&account_id, Some("inbox"), false, 0, 2)
        .await;
    assert_eq!(subjects(&first), ["Newest", "Middle"]);
    assert_eq!(first.total, 3, "total is the whole folder, not the page");

    let second = app
        .query_messages(&account_id, Some("inbox"), false, 2, 2)
        .await;
    assert_eq!(subjects(&second), ["Oldest"]);
    assert_eq!(second.offset, 2, "the page echoes where it started");
}

#[tokio::test]
async fn the_page_limit_is_bounded_one_to_fifty() {
    let provider = fakes::FakeProvider::with_mail(
        tree(),
        vec![
            delivered_at(mail("m1", "inbox", "One"), 9, 0),
            delivered_at(mail("m2", "inbox", "Two"), 10, 0),
        ],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    let zero = app
        .query_messages(&account_id, Some("inbox"), false, 0, 0)
        .await;
    assert_eq!(
        zero.rows.len(),
        1,
        "a limit of zero still reads one row, never an empty query",
    );

    let over = app
        .query_messages(&account_id, Some("inbox"), false, 0, MAX_PAGE + 100)
        .await;
    assert_eq!(
        over.rows.len(),
        2,
        "a limit over {MAX_PAGE} reads at most the bound (here only two exist)",
    );
    assert_eq!(over.total, 2);
}

#[tokio::test]
async fn unread_only_narrows_the_total_as_well_as_the_rows() {
    // A `total` that counted read mail too would make "you have 3 unread" a
    // lie the moment a caller trusted it.
    let already = read(mail("m2", "inbox", "Already read"));
    let provider =
        fakes::FakeProvider::with_mail(tree(), vec![mail("m1", "inbox", "Unread"), already]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    let page = app
        .query_messages(&account_id, Some("inbox"), true, 0, 20)
        .await;
    assert_eq!(subjects(&page), ["Unread"]);
    assert_eq!(page.total, 1);
}

#[tokio::test]
async fn a_search_defaults_to_every_folder_except_trash_and_reaches_it_when_narrowed() {
    // The search contract's own rules, inherited rather than reimplemented:
    // the default scope skips Trash, and narrowing to it is how you search it.
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

    let wide = app.query_search("Invoice", None, None, 0, 20).await;
    assert_eq!(
        subjects(&wide),
        ["Invoice for March"],
        "the trashed invoice is out of the default scope",
    );

    let narrowed = app
        .query_search("Invoice", Some(&account_id), Some("trash"), 0, 20)
        .await;
    assert_eq!(
        subjects(&narrowed),
        ["Invoice for February"],
        "and reachable by naming the folder beside its account",
    );
}

#[tokio::test]
async fn a_search_of_nothing_answers_nothing() {
    // A query of nothing but whitespace names nobody: the honest answer is no
    // rows, never the whole mailbox an empty engine query would match.
    let provider = fakes::FakeProvider::with_mail(tree(), vec![mail("m1", "inbox", "Hello")]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let page = app.query_search("   ", None, None, 0, 20).await;
    assert_eq!(subjects(&page), Vec::<String>::new());
    assert_eq!(page.total, 0);
}

#[tokio::test]
async fn query_accounts_names_every_configured_account() {
    let provider = fakes::FakeProvider::with_mail(tree(), Vec::new());
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);

    let accounts = app.query_accounts().await;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id.as_str(), "acct-1");
    assert_eq!(accounts[0].address, "me@acct-1.local");
}

#[tokio::test]
async fn query_folders_answers_the_sidebar_order() {
    // The same canonical order the pane shows, so an agent and a person name
    // the same folder the same way.
    let provider = fakes::FakeProvider::with_mail(tree(), Vec::new());
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account_id = AccountId::try_from("acct-1").unwrap();

    let folders = app.query_folders(&account_id).await;
    let keys: Vec<&str> = folders.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, ["inbox", "trash", "archive"]);
}

#[tokio::test]
async fn query_message_answers_headers_plain_body_and_attachment_names_only() {
    // A source with one text/plain part and one pdf attachment: the body is
    // the engine's own plain conversion, the attachments are names only, and
    // the flags say unread because reading through this door changes nothing.
    let source = concat!(
        "Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n",
        "--mix\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n",
        "The numbers are in.\r\n",
        "--mix\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\n",
        "Content-Disposition: attachment; filename=\"report.pdf\"\r\n",
        "Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n",
        "--mix--\r\n",
    );
    let flagged_unread = flagged(mail("m1", "inbox", "Quarterly report"));
    let provider = fakes::FakeProvider::with_sources(
        tree(),
        vec![delivered_at(flagged_unread, 8, 30)],
        vec![("m1".to_owned(), source.as_bytes().to_vec())],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let detail = app
        .query_message(&msg("acct-1", "m1"))
        .await
        .expect("the message resolves");

    assert_eq!(detail.account.as_str(), "acct-1");
    assert_eq!(detail.key.as_str(), "m1");
    assert_eq!(detail.subject, "Quarterly report");
    assert_eq!(detail.from, "Ada Lovelace <ada@example.test>");
    assert_eq!(detail.date, "2026-10-09T08:30:00Z");
    assert!(detail.unread);
    assert!(detail.flagged);
    assert_eq!(detail.body_text, "The numbers are in.");
    assert_eq!(detail.attachment_names, ["report.pdf"]);
    assert!(!detail.load_error);
}

#[tokio::test]
async fn query_message_reports_a_body_that_could_not_be_fetched() {
    // A message with no source registered: the headers still answer, the body
    // honestly reports a load error rather than claiming the message has no
    // body at all.
    let provider =
        fakes::FakeProvider::with_mail(tree(), vec![mail("m1", "inbox", "No source here")]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let detail = app
        .query_message(&msg("acct-1", "m1"))
        .await
        .expect("the message resolves");
    assert_eq!(detail.subject, "No source here");
    assert!(
        detail.load_error,
        "the fetch failure is stated, not folded into an empty body"
    );
    assert_eq!(detail.body_text, "");
}

#[tokio::test]
async fn query_message_answers_nothing_for_a_message_the_store_does_not_hold() {
    let provider = fakes::FakeProvider::with_mail(tree(), vec![mail("m1", "inbox", "Hello")]);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app(vec![account("acct-1", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert!(
        app.query_message(&msg("acct-1", "gone")).await.is_none(),
        "a key the account's synced set does not hold resolves to nothing",
    );
}

/// The subjects of a page's rows, in order; the shape every listing assertion
/// reads before it looks at any row's fields.
fn subjects(page: &MessagePage) -> Vec<String> {
    page.rows.iter().map(|row| row.subject.clone()).collect()
}
