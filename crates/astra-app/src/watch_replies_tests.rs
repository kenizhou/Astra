// SPDX-License-Identifier: GPL-3.0-only
//! Core watch-replies tests over the dispatch fakes: a watched reply fires
//! and consumes the watch, the RE-KEYING REGRESSION (a reply whose Message-ID
//! sorts BEFORE the original's still fires, because the check re-resolves the
//! member key through the engine), the preference off consumes nothing, an
//! unwatched conversation raises nothing, a second reply after the consume
//! raises nothing, a stale member link fails open, the provider-threaded leg,
//! the thread-row member capture, and the local-only red line (no engine
//! write, the mail unmoved). The in-memory engine derives threads at apply,
//! so the check resolves for real.

use std::sync::{Arc, Mutex};

use astra_watches::{NewWatch, WatchStore};
use engine_api::{Message, MessageIdHeader};

use crate::{
    dispatch_fakes as fakes,
    protocol::SettingsIntent,
    reference::{MessageRef, RowRef, ThreadRef},
    watch_replies::WatchError,
    Intent,
};

use fakes::{account, app, delivered_at, inbox, mail, threaded};

/// One app over one "work" account holding `messages`, a watch store
/// installed, synced and ready. Answers the app and its store.
async fn setup(messages: Vec<Message>) -> (crate::App<fakes::FakeProvider>, Arc<WatchStore>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], messages);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = Arc::new(WatchStore::open_in_memory().unwrap());
    app.install_watch_store(Arc::clone(&store));
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, store)
}

/// Stamps `message` with its own Message-ID, the id the engine derives the
/// thread from.
fn with_msgid(mut message: Message, id: &str) -> Message {
    message.envelope.message_id = vec![MessageIdHeader::new(id).unwrap()];
    message
}

/// A reply `key` about `subject`, owning `msgid` and answering `in_reply_to`
/// (its In-Reply-To and References both name the original).
fn reply(key: &str, subject: &str, msgid: &str, in_reply_to: &str) -> Message {
    let mut message = mail(key, "inbox", subject);
    message.envelope.message_id = vec![MessageIdHeader::new(msgid).unwrap()];
    message.envelope.in_reply_to = vec![MessageIdHeader::new(in_reply_to).unwrap()];
    message.envelope.references = vec![MessageIdHeader::new(in_reply_to).unwrap()];
    message
}

/// A message row on "work" under `key`.
fn message_row(key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts("work", key.to_owned()).unwrap())
}

/// A thread row on "work" under `thread`.
fn thread_row(thread: &str) -> RowRef {
    RowRef::Thread(ThreadRef::from_parts("work", thread.to_owned()).unwrap())
}

/// Delivers `message` server-side, so the account's next pass presents it.
async fn deliver(app: &crate::App<fakes::FakeProvider>, message: Message) {
    let provider = &app
        .account_handle(&engine_api::AccountId::try_from("work").unwrap())
        .await
        .expect("the work account stands")
        .providers[0];
    provider.messages.lock().unwrap().push(message.clone());
    provider.updates.lock().unwrap().push(message);
}

/// Runs one pass, drains the batch, and answers the reply check's notices.
async fn pass_and_check(app: &crate::App<fakes::FakeProvider>) -> Vec<crate::NewMailNotice> {
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let batch = app.drain_new_mail();
    app.check_watched_replies(&batch.messages).await
}

#[tokio::test]
async fn a_watched_reply_fires_and_consumes_the_watch() {
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let (app, store) = setup(vec![original]).await;
    assert!(app.watch_thread(message_row("m1")).await.unwrap());
    assert_eq!(store.list().unwrap().len(), 1);

    let answer = reply(
        "m2",
        "Re: the original",
        "zzz-reply@example.test",
        "aaa-original@example.test",
    );
    deliver(&app, answer).await;
    let notices = pass_and_check(&app).await;

    assert_eq!(notices.len(), 1, "the first reply raises one notice");
    assert_eq!(notices[0].key.as_str(), "m2", "the notice names the reply");
    assert_eq!(notices[0].subject.as_deref(), Some("Re: the original"));
    assert!(
        store.list().unwrap().is_empty(),
        "the one-shot consume clears the watch"
    );
}

#[tokio::test]
async fn a_reply_whose_message_id_sorts_first_still_fires() {
    // The RE-KEYING REGRESSION: the original owns the LARGER Message-ID, so
    // the derived thread is named after it at set time; the reply owns the
    // SMALLER one, so the engine re-keys the whole component onto the reply's
    // id when it lands. A watch matched on the stored thread id would miss;
    // the member-key re-resolution matches resolved-against-resolved and
    // fires anyway.
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "zzz-original@example.test",
    );
    let (app, store) = setup(vec![original]).await;
    app.watch_thread(message_row("m1")).await.unwrap();
    // The watch's handle is the thread id resolved at set time: the original's
    // own (larger) Message-ID, about to be re-keyed out from under it.
    assert_eq!(
        store.list().unwrap()[0].thread_id,
        "zzz-original@example.test"
    );

    let answer = reply(
        "m2",
        "Re: the original",
        "aaa-reply@example.test",
        "zzz-original@example.test",
    );
    deliver(&app, answer).await;
    let notices = pass_and_check(&app).await;

    assert_eq!(
        notices.len(),
        1,
        "the re-keyed thread still matches through the member key"
    );
    assert_eq!(notices[0].key.as_str(), "m2");
    assert!(store.list().unwrap().is_empty(), "the watch consumes");
}

#[tokio::test]
async fn the_preference_off_raises_no_notice_and_consumes_nothing() {
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let (app, store) = setup(vec![original]).await;
    app.dispatch(Intent::Settings(SettingsIntent::SetWatchRepliesEnabled {
        enabled: false,
    }))
    .await
    .unwrap();
    app.watch_thread(message_row("m1")).await.unwrap();

    deliver(
        &app,
        reply(
            "m2",
            "Re: the original",
            "zzz-reply@example.test",
            "aaa-original@example.test",
        ),
    )
    .await;
    let notices = pass_and_check(&app).await;

    assert!(notices.is_empty(), "the switch off raises no reply notice");
    assert_eq!(
        store.list().unwrap().len(),
        1,
        "the switch off consumes nothing: the watch stays armed"
    );
}

#[tokio::test]
async fn an_unwatched_conversation_raises_nothing() {
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let (app, _store) = setup(vec![original]).await;
    // No watch set.
    deliver(
        &app,
        reply(
            "m2",
            "Re: the original",
            "zzz-reply@example.test",
            "aaa-original@example.test",
        ),
    )
    .await;
    let notices = pass_and_check(&app).await;
    assert!(
        notices.is_empty(),
        "an unwatched conversation raises nothing"
    );
}

#[tokio::test]
async fn a_second_reply_after_the_consume_raises_nothing() {
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let (app, store) = setup(vec![original]).await;
    app.watch_thread(message_row("m1")).await.unwrap();

    deliver(
        &app,
        reply(
            "m2",
            "Re: the original",
            "bbb-reply@example.test",
            "aaa-original@example.test",
        ),
    )
    .await;
    assert_eq!(pass_and_check(&app).await.len(), 1, "the first reply fires");
    assert!(
        store.list().unwrap().is_empty(),
        "the first reply consumes the watch"
    );

    deliver(
        &app,
        reply(
            "m3",
            "Re: the original",
            "ccc-reply@example.test",
            "aaa-original@example.test",
        ),
    )
    .await;
    assert!(
        pass_and_check(&app).await.is_empty(),
        "the watch is one-shot: a second reply raises nothing"
    );
}

#[tokio::test]
async fn a_stale_member_link_fails_open_and_keeps_the_watch() {
    // A watch whose representative key names nothing the store holds (the
    // message was moved or deleted) cannot resolve: the check skips it,
    // raises nothing, and leaves the watch armed but silent, never stranding
    // the pass.
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let (app, store) = setup(vec![original]).await;
    store
        .watch(&NewWatch {
            account: engine_api::AccountId::try_from("work").unwrap(),
            thread_id: "aaa-original@example.test".to_owned(),
            member_key: engine_api::ProviderKey::new("ghost").unwrap(),
        })
        .unwrap();

    deliver(
        &app,
        reply(
            "m2",
            "Re: the original",
            "zzz-reply@example.test",
            "aaa-original@example.test",
        ),
    )
    .await;
    let notices = pass_and_check(&app).await;

    assert!(notices.is_empty(), "a stale link raises nothing");
    assert_eq!(
        store.list().unwrap().len(),
        1,
        "the watch stays armed: fail-open never consumes on a miss"
    );
}

#[tokio::test]
async fn the_provider_threaded_leg_fires() {
    // A provider-assigned thread (the JMAP shape): the thread id is the
    // provider's, never re-keyed, and the check resolves both sides onto it.
    let (app, store) = setup(vec![threaded(mail("m1", "inbox", "the original"), "th-1")]).await;
    app.watch_thread(thread_row("th-1")).await.unwrap();

    deliver(
        &app,
        threaded(mail("m2", "inbox", "Re: the original"), "th-1"),
    )
    .await;
    let notices = pass_and_check(&app).await;

    assert_eq!(notices.len(), 1, "the provider-threaded reply fires");
    assert_eq!(notices[0].key.as_str(), "m2");
    assert!(
        store.list().unwrap().is_empty(),
        "the provider-threaded watch consumes"
    );
}

#[tokio::test]
async fn a_thread_row_watch_captures_the_newest_member() {
    let (app, store) = setup(vec![
        threaded(delivered_at(mail("m1", "inbox", "older"), 8, 0), "th-1"),
        threaded(delivered_at(mail("m2", "inbox", "newer"), 9, 0), "th-1"),
    ])
    .await;
    app.watch_thread(thread_row("th-1")).await.unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].member_key.as_str(),
        "m2",
        "the thread row captures its newest member as the representative link"
    );
}

#[tokio::test]
async fn unwatch_clears_the_watch_idempotently() {
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let (app, store) = setup(vec![original]).await;
    app.watch_thread(message_row("m1")).await.unwrap();

    assert!(app.unwatch_thread(message_row("m1")).await.unwrap());
    assert!(
        store.list().unwrap().is_empty(),
        "the unwatch clears the row"
    );
    assert!(
        !app.unwatch_thread(message_row("m1")).await.unwrap(),
        "a second unwatch removes nothing"
    );
}

#[tokio::test]
async fn the_watch_never_moves_mail_or_writes_the_engine() {
    // The local-only red line: no edit_mail, no outbox op, no folder edit,
    // and both messages stay in the inbox.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let original = with_msgid(
        mail("m1", "inbox", "the original"),
        "aaa-original@example.test",
    );
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![original]);
    let edits = Arc::clone(&provider.edits);
    let submits = Arc::clone(&provider.submits);
    let mailbox_edits = Arc::clone(&provider.mailbox_edits);
    let app = app(vec![account("work", provider)], &surfaces);
    let store = Arc::new(WatchStore::open_in_memory().unwrap());
    app.install_watch_store(Arc::clone(&store));
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.watch_thread(message_row("m1")).await.unwrap();
    deliver(
        &app,
        reply(
            "m2",
            "Re: the original",
            "zzz-reply@example.test",
            "aaa-original@example.test",
        ),
    )
    .await;
    pass_and_check(&app).await;
    app.unwatch_thread(message_row("m1")).await.unwrap();

    assert!(edits.lock().unwrap().is_empty(), "no edit_mail queued");
    assert!(submits.lock().unwrap().is_empty(), "no outbox op queued");
    assert!(
        mailbox_edits.lock().unwrap().is_empty(),
        "no folder edit queued"
    );
    let stored = app
        .engine
        .mail_window(
            &[engine_api::AccountId::try_from("work").unwrap()],
            usize::MAX,
        )
        .await
        .unwrap();
    assert_eq!(stored.len(), 2, "both messages stay in the store, unmoved");
    assert!(
        stored
            .iter()
            .all(|row| row.mailboxes.iter().any(|m| m.as_str() == "inbox")),
        "both messages stay in the inbox"
    );
}

#[tokio::test]
async fn a_storeless_app_watches_nothing_and_refuses_writes() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox()], vec![mail("m1", "inbox", "one")]);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert!(matches!(
        app.watch_thread(message_row("m1")).await,
        Err(WatchError::NoStore)
    ));
    assert!(
        app.list_watches().unwrap().is_empty(),
        "no store, no watches"
    );
    assert!(
        app.check_watched_replies(&[]).await.is_empty(),
        "no store, the check answers nothing"
    );
}
