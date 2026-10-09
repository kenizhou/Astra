// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading pipeline and its dispatch arms: the snapshot one open
//! renders from a stored source, the mark-read that rides the open, the close
//! that clears it, and the refresh that republishes fresh content or closes on a
//! message the server no longer holds. The fixtures live in `dispatch_fakes`, the
//! crate-root test module every dispatch-shaped test shares.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::{dispatch_fakes as fakes, reference::MessageRef, Intent, Surface};

use fakes::{account, app, delivered_at, folder, inbox, mail, read};

/// The rich message every reading fixture opens: an HTML body carrying one inline
/// `cid:` image and one remote image, the png part the reference resolves to, and
/// one pdf attachment. Base64: `aGVsbG8=` is `hello`, `UERG` is `PDF`.
fn rich_source() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(b"Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n");
    raw.extend_from_slice(b"--mix\r\nContent-Type: multipart/related; boundary=\"rel\"\r\n\r\n");
    raw.extend_from_slice(
        b"--rel\r\nContent-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n",
    );
    raw.extend_from_slice(
        b"--alt\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
          The launch is ready.\r\n",
    );
    raw.extend_from_slice(
        b"--alt\r\nContent-Type: text/html; charset=\"utf-8\"\r\n\r\n\
          <p>The launch is ready.</p><img src=\"cid:logo@x\">\
          <img src=\"https://tracker.example/p.png\">\r\n",
    );
    raw.extend_from_slice(b"--alt--\r\n");
    raw.extend_from_slice(
        b"--rel\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\n\
          Content-Disposition: inline\r\nContent-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n",
    );
    raw.extend_from_slice(b"--rel--\r\n");
    raw.extend_from_slice(
        b"--mix\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n",
    );
    raw.extend_from_slice(b"--mix--\r\n");
    raw
}

/// An app over one account whose Inbox holds the unread rich message `m1`, synced
/// and selected, so a reading open has rows, a store, and a source to read. Also
/// hands back the provider's edit log, the write path's witness.
async fn rich_mailbox(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> (
    crate::App<fakes::FakeProvider>,
    Arc<Mutex<Vec<engine_provider::MailEdit>>>,
) {
    let provider = fakes::FakeProvider::with_sources(
        vec![inbox(), folder("archive", "Archive")],
        vec![delivered_at(mail("m1", "inbox", "Launch"), 8, 0)],
        vec![("m1".to_owned(), rich_source())],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::SelectFolder {
        folder: crate::FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    (app, edits)
}

/// The reference to the rich message, the message every open fixture opens.
fn m1() -> MessageRef {
    MessageRef::from_parts("work", "m1".to_owned()).unwrap()
}

#[tokio::test]
async fn open_message_publishes_the_sanitised_body_and_marks_read() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, edits) = rich_mailbox(&surfaces).await;
    assert!(
        fakes::row(&app.mailbox_list(), "m1").unread,
        "starts unread"
    );

    app.dispatch(Intent::OpenMessage { message: m1() })
        .await
        .unwrap();

    let snapshot = app.reading().expect("the open message's body");
    assert_eq!(snapshot.message.key.as_str(), "m1");
    assert_eq!(snapshot.message.account.as_str(), "work");
    assert_eq!(snapshot.subject, "Launch");
    assert_eq!(snapshot.from_name, "Ada Lovelace");
    assert_eq!(snapshot.from_address, "ada@example.test");
    assert_eq!(snapshot.sent_at, "2026-10-09T08:00:00Z");
    // The inline cid: image resolved into the body; the remote image became a
    // placeholder and is counted; the sender's own markup survives.
    let html = snapshot.html.as_deref().expect("an html part");
    assert!(
        html.contains("src=\"data:image/png;base64,aGVsbG8=\""),
        "{html}"
    );
    assert!(
        html.contains(r#"data-astra-src="https://tracker.example/p.png""#),
        "{html}"
    );
    assert!(!html.contains(r#" src="http"#), "{html}");
    assert!(html.contains("<p>The launch is ready.</p>"), "{html}");
    let plain = snapshot.plain.as_deref().expect("a plain part");
    assert!(plain.contains("The launch is ready."), "{plain}");
    assert_eq!(snapshot.remote_image_count, 1);
    assert_eq!(snapshot.attachments.len(), 1);
    assert_eq!(snapshot.attachments[0].file_name, "report.pdf");
    assert_eq!(snapshot.attachments[0].media_type, "application/pdf");
    assert_eq!(snapshot.attachments[0].size, 3);
    assert!(
        surfaces.lock().unwrap().contains(&Surface::Reading),
        "the reading surface is signalled"
    );
    // The open marks the message read through the list's own write path.
    assert!(!fakes::row(&app.mailbox_list(), "m1").unread);
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_provider::MailEdit::mark_seen(
            engine_api::ProviderKey::new("m1").unwrap(),
            true,
        )],
        "one keyword edit through the provider, exactly as the list's mark-read sends it"
    );

    // The read state survives a refresh, and the open reading stays open.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(!fakes::row(&app.mailbox_list(), "m1").unread);
    assert_eq!(
        app.reading().expect("still open").message.key.as_str(),
        "m1"
    );
}

#[tokio::test]
async fn open_message_skips_the_write_for_an_already_read_message() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![inbox()],
        vec![read(delivered_at(mail("m1", "inbox", "Launch"), 8, 0))],
        vec![("m1".to_owned(), rich_source())],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::OpenMessage { message: m1() })
        .await
        .unwrap();

    assert!(app.reading().is_some(), "the body still opens");
    assert!(
        edits.lock().unwrap().is_empty(),
        "an already-read message is not edited again"
    );
}

#[tokio::test]
async fn open_message_of_an_unknown_message_leaves_the_pane_empty() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = rich_mailbox(&surfaces).await;

    let missing = MessageRef::from_parts("work", "m-none".to_owned()).unwrap();
    app.dispatch(Intent::OpenMessage { message: missing })
        .await
        .unwrap();

    assert_eq!(app.reading(), None, "nothing opens for an unknown message");
}

#[tokio::test]
async fn close_reading_clears_the_surface_and_a_refresh_does_not_resurrect_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = rich_mailbox(&surfaces).await;
    app.dispatch(Intent::OpenMessage { message: m1() })
        .await
        .unwrap();
    assert!(app.reading().is_some());

    app.dispatch(Intent::CloseReading).await.unwrap();

    assert_eq!(app.reading(), None, "the pane is cleared");
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        app.reading(),
        None,
        "a refresh republishes nothing once the pane is closed"
    );
}

#[tokio::test]
async fn refresh_republishes_the_open_reading_with_fresh_content() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![inbox()],
        vec![delivered_at(mail("m1", "inbox", "Launch"), 8, 0)],
        vec![("m1".to_owned(), rich_source())],
    );
    let updates = Arc::clone(&provider.updates);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::OpenMessage { message: m1() })
        .await
        .unwrap();
    assert_eq!(app.reading().unwrap().subject, "Launch");
    let signals = surfaces.lock().unwrap().len();

    // The server moves the message's subject under the client: the next pass
    // delivers it as a delta, the way a real refresh would.
    updates
        .lock()
        .unwrap()
        .push(delivered_at(mail("m1", "inbox", "Launch (updated)"), 8, 0));
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let snapshot = app.reading().expect("the open reading republished");
    assert_eq!(
        snapshot.subject, "Launch (updated)",
        "the pipeline re-ran over the store's fresh copy"
    );
    assert!(
        surfaces.lock().unwrap().len() > signals,
        "the republished reading is signalled"
    );
}

#[tokio::test]
async fn refresh_closes_the_reading_when_the_message_is_gone() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![inbox()],
        vec![delivered_at(mail("m1", "inbox", "Launch"), 8, 0)],
        vec![("m1".to_owned(), rich_source())],
    );
    let messages = Arc::clone(&provider.messages);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::OpenMessage { message: m1() })
        .await
        .unwrap();
    assert!(app.reading().is_some());

    // The message is deleted server-side: the next pass re-presents a present set
    // without it, so the reconcile drops it from the store.
    messages.lock().unwrap().clear();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert_eq!(
        app.reading(),
        None,
        "the reading closes on the gone message"
    );
}

#[tokio::test]
async fn render_reading_answers_none_for_a_message_the_store_does_not_hold() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, _edits) = rich_mailbox(&surfaces).await;
    let work = AccountId::try_from("work").unwrap();
    let handle = app.account_handle(&work).await.expect("the account");
    let provider = handle.providers.first().expect("a provider");

    let gone = MessageRef::from_parts("work", "m-none".to_owned()).unwrap();
    assert!(
        super::render_reading(&app.engine, provider, &gone)
            .await
            .is_none(),
        "an unresolved message renders nothing"
    );
}

#[tokio::test]
async fn the_sender_name_falls_back_to_the_bare_address() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut unnamed = delivered_at(mail("m1", "inbox", "Launch"), 8, 0);
    unnamed.envelope.from = vec![engine_api::EmailAddress::new("plain@example.test")];
    let provider = fakes::FakeProvider::with_sources(
        vec![inbox()],
        vec![unnamed],
        vec![("m1".to_owned(), rich_source())],
    );
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::OpenMessage { message: m1() })
        .await
        .unwrap();

    let snapshot = app.reading().expect("the body");
    assert_eq!(snapshot.from_name, "plain@example.test");
    assert_eq!(snapshot.from_address, "plain@example.test");
}
