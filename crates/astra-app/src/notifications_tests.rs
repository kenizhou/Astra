// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the new-mail queue: the qualification rules (new only,
//! inbox-class only, not the account's own filing), the bound and its
//! overflow count, the drain's clearing, and the first import's silence,
//! all driven over the dispatch fakes the way the pass paths drive them.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, EmailAddress};

use crate::dispatch_fakes as fakes;
use crate::{App, Intent};

/// A provider holding `messages` in a plain account tree (an Inbox, an
/// Archive, a Sent, and a Trash), the shape every queue test stands on.
fn provider(messages: Vec<engine_api::Message>) -> fakes::FakeProvider {
    fakes::FakeProvider::with_mail(
        vec![
            fakes::inbox(),
            fakes::folder("archive", "Archive"),
            fakes::sent(),
            fakes::trash(),
        ],
        messages,
    )
}

/// An app over one work account holding `messages`.
fn app(
    messages: Vec<engine_api::Message>,
) -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<crate::Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let built = fakes::app(vec![fakes::account("work", provider(messages))], &surfaces);
    (built, surfaces)
}

/// The work account's id, the way every assertion names it.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// Delivers `message` server-side, so the account's next pass presents it
/// the way a real server's delta would.
async fn deliver(app: &App<fakes::FakeProvider>, message: engine_api::Message) {
    let provider = &app
        .account_handle(&work())
        .await
        .expect("the work account stands")
        .providers[0];
    provider.messages.lock().unwrap().push(message.clone());
    provider.updates.lock().unwrap().push(message);
}

#[tokio::test]
async fn a_pass_holds_a_new_inbox_row_for_the_host() {
    let (app, _surfaces) = app(vec![fakes::mail("m1", "inbox", "the first import")]);
    // The first pass is the import: it arms the queue's known set, so it
    // must not notify. The row that arrives after it is the new mail.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let mut m2 = fakes::mail("m2", "inbox", "the launch monitor");
    m2.preview = Some("how the launch went".to_owned());
    deliver(&app, m2).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let batch = app.drain_new_mail();
    assert_eq!(batch.overflow, 0);
    assert_eq!(batch.messages.len(), 1, "one new row, one notice");
    let notice = &batch.messages[0];
    assert_eq!(notice.account, work());
    assert_eq!(notice.key.as_str(), "m2");
    assert_eq!(
        notice.from.as_ref().map(|from| from.email.as_str()),
        Some("ada@example.test"),
        "who it is from"
    );
    assert_eq!(notice.subject.as_deref(), Some("the launch monitor"));
    assert_eq!(notice.preview.as_deref(), Some("how the launch went"));
}

#[tokio::test]
async fn a_changed_row_does_not_qualify() {
    let (app, _surfaces) = app(vec![fakes::mail("m1", "inbox", "monitor the launch")]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    // A flag change another device made: the same key, re-delivered whole.
    deliver(
        &app,
        fakes::read(fakes::mail("m1", "inbox", "monitor the launch")),
    )
    .await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let batch = app.drain_new_mail();
    assert!(
        batch.messages.is_empty(),
        "a row the store already held is a change, not new mail"
    );
    assert_eq!(batch.overflow, 0);
}

#[tokio::test]
async fn a_row_outside_the_inbox_does_not_qualify() {
    let (app, _surfaces) = app(vec![fakes::mail("m1", "inbox", "the first import")]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    deliver(&app, fakes::mail("m2", "archive", "filed on arrival")).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let batch = app.drain_new_mail();
    assert!(
        batch.messages.is_empty(),
        "new mail outside the inbox-class folders is not announced"
    );
}

#[tokio::test]
async fn the_accounts_own_filing_does_not_qualify() {
    let (app, _surfaces) = app(vec![fakes::mail("m1", "inbox", "the first import")]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    // A provider that files the account's own send in the Inbox, the way a
    // label-shaped server does: new to the store, but the person sent it.
    let mut own = fakes::mail("m2", "inbox", "my own send");
    own.envelope.from = vec![EmailAddress::new("me@work.local")];
    deliver(&app, own).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let batch = app.drain_new_mail();
    assert!(
        batch.messages.is_empty(),
        "the sent filing is not new mail even in the inbox"
    );
}

#[tokio::test]
async fn overflow_folds_into_a_count() {
    let (app, _surfaces) = app(vec![fakes::mail("seed", "inbox", "the first import")]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    for index in 0..6 {
        deliver(
            &app,
            fakes::mail(&format!("m{index}"), "inbox", "one of a folder's worth"),
        )
        .await;
    }
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let batch = app.drain_new_mail();
    assert_eq!(
        batch.messages.len(),
        crate::notifications::NEW_MAIL_QUEUE_CAP,
        "the queue holds a handful"
    );
    assert_eq!(batch.overflow, 1, "the rest folds into a count");
}

#[tokio::test]
async fn draining_clears_the_batch() {
    let (app, _surfaces) = app(vec![fakes::mail("m1", "inbox", "the first import")]);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    deliver(&app, fakes::mail("m2", "inbox", "monitor the launch")).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert_eq!(app.drain_new_mail().messages.len(), 1);
    let again = app.drain_new_mail();
    assert!(again.messages.is_empty(), "the drain takes the whole batch");
    assert_eq!(again.overflow, 0);
}

#[tokio::test]
async fn the_first_import_is_not_new_mail() {
    let (app, _surfaces) = app(vec![
        fakes::mail("m1", "inbox", "history one"),
        fakes::mail("m2", "inbox", "history two"),
        fakes::mail("m3", "inbox", "history three"),
    ]);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let batch = app.drain_new_mail();
    assert!(
        batch.messages.is_empty(),
        "a cold store's first pass is an import, not a delivery"
    );
    assert_eq!(batch.overflow, 0);
}
