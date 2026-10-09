// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the compose family's arm: the prefill each kind computes from the
//! original's headers per mail-calendar's reply rules (recipients with the
//! sender's own identity excluded, subject prefixes that do not double, the
//! threading header a reply carries and a forward does not), the quoted original
//! with its attribution line, and the one publish per dispatch the composer
//! hydrates from. The fixtures live in `dispatch_fakes`, the crate-root test
//! module every dispatch-shaped test shares.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, Quote};
use astra_viewmodel::ComposeKind;
use engine_api::{AccountId, EmailAddress, Message, MessageBody, MessageIdHeader};

use crate::{
    compose_request::compose_request, dispatch_fakes as fakes, ComposeIntent, Intent, Surface,
};

/// A well-formed account id, the way the compose tests name one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// The original every compose fixture answers: Ada writing to the account's own
/// identity and Bob, cc'ing Carol, under her own message-id, delivered at the
/// fixture base instant.
fn original(key: &str, subject: &str) -> Message {
    let mut message = fakes::mail(key, "inbox", subject);
    message.envelope.to = vec![
        EmailAddress::new("me@work.local"),
        EmailAddress::named("Bob", "bob@example.test"),
    ];
    message.envelope.cc = vec![EmailAddress::new("carol@example.test")];
    message.envelope.message_id = vec![MessageIdHeader::new("orig-1@server.test").unwrap()];
    message
}

/// The original's source: two lines of plain text the quote carries.
fn plain_source() -> Vec<u8> {
    b"Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
      The numbers are in.\r\n\
      Second line.\r\n"
        .to_vec()
}

/// An app over one `work` account (identity `me@work.local`) whose Inbox holds
/// `original` and its `source`, synced so the store resolves it.
async fn app_over(
    original: Message,
    source: Option<Vec<u8>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> crate::App<fakes::FakeProvider> {
    let provider = match source {
        Some(source) => fakes::FakeProvider::with_sources(
            vec![fakes::inbox()],
            vec![original],
            vec![("m1".to_owned(), source)],
        ),
        None => fakes::FakeProvider::with_mail(vec![fakes::inbox()], vec![original]),
    };
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// The reference to the original, the message every fixture answers.
fn m1() -> crate::MessageRef {
    crate::MessageRef::from_parts("work", "m1".to_owned()).unwrap()
}

/// The compose family's intent for `kind` over the original.
fn compose(kind: ComposeKind) -> Intent {
    let message = m1();
    Intent::Compose(match kind {
        ComposeKind::Reply => ComposeIntent::Reply { message },
        ComposeKind::ReplyAll => ComposeIntent::ReplyAll { message },
        ComposeKind::Forward => ComposeIntent::Forward { message },
    })
}

/// The snapshot's single quote block, the assertion's own witness that the
/// document carries exactly the quoted original.
fn quote_of(snapshot: &astra_viewmodel::ComposeRequestSnapshot) -> &Quote {
    match snapshot.document.blocks.as_slice() {
        [Block::Quote(quote)] => quote,
        blocks => panic!("one quote block, not {blocks:?}"),
    }
}

#[tokio::test]
async fn reply_prefills_the_sender_the_threading_and_the_quoted_original() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(
        original("m1", "Quarterly report"),
        Some(plain_source()),
        &surfaces,
    )
    .await;

    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();

    let snapshot = app.compose_request().expect("the reply's prefill");
    assert_eq!(snapshot.kind, ComposeKind::Reply);
    assert_eq!(snapshot.from, account("work"));
    assert_eq!(snapshot.to, "ada@example.test");
    assert_eq!(snapshot.cc, "", "a plain reply leaves Cc to the user");
    assert_eq!(snapshot.subject, "Re: Quarterly report");
    assert_eq!(snapshot.in_reply_to.as_deref(), Some("orig-1@server.test"));
    let quote = quote_of(&snapshot);
    assert_eq!(
        quote.attribution.line,
        "On 9 Oct 2026 at 08:00, Ada Lovelace <ada@example.test> wrote:"
    );
    assert_eq!(
        quote.attribution.headers,
        vec![
            astra_composer::QuoteHeader {
                label: "From".to_owned(),
                value: "Ada Lovelace <ada@example.test>".to_owned(),
            },
            astra_composer::QuoteHeader {
                label: "Sent".to_owned(),
                value: "9 Oct 2026 at 08:00".to_owned(),
            },
            astra_composer::QuoteHeader {
                label: "To".to_owned(),
                value: "me@work.local, Bob <bob@example.test>".to_owned(),
            },
            astra_composer::QuoteHeader {
                label: "Cc".to_owned(),
                value: "carol@example.test".to_owned(),
            },
            astra_composer::QuoteHeader {
                label: "Subject".to_owned(),
                value: "Quarterly report".to_owned(),
            },
        ],
    );
    assert!(
        quote.body_plain.contains("The numbers are in."),
        "{}",
        quote.body_plain
    );
    assert!(
        quote.body_plain.contains("Second line."),
        "the quote keeps the original's own line breaks"
    );
    assert_eq!(
        quote.body_html,
        format!(
            "<div style=\"white-space: pre-wrap;\">{}</div>",
            quote.body_plain
        ),
        "the quote's html is its own plain text, escaped, in the one wrapper"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::ComposeRequest),
        "the compose-request surface is signalled"
    );
}

#[tokio::test]
async fn reply_all_preserves_the_original_layout_minus_the_account_itself() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(
        original("m1", "Quarterly report"),
        Some(plain_source()),
        &surfaces,
    )
    .await;

    app.dispatch(compose(ComposeKind::ReplyAll)).await.unwrap();

    let snapshot = app.compose_request().expect("the reply-all's prefill");
    assert_eq!(snapshot.kind, ComposeKind::ReplyAll);
    // The sender leads, the original's other To recipient stays in To, its Cc
    // stays Cc, and the account's own identity is in neither.
    assert_eq!(snapshot.to, "ada@example.test, bob@example.test");
    assert_eq!(snapshot.cc, "carol@example.test");
}

#[tokio::test]
async fn a_subject_already_prefixed_is_not_prefixed_again() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(
        original("m1", "Re: Quarterly report"),
        Some(plain_source()),
        &surfaces,
    )
    .await;

    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();
    assert_eq!(
        app.compose_request().unwrap().subject,
        "Re: Quarterly report",
        "the existing prefix is preserved, not doubled"
    );

    // The check is case-insensitive: a sender's `RE:` is still a reply prefix.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(original("m1", "RE: Quarterly report"), None, &surfaces).await;
    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();
    assert_eq!(
        app.compose_request().unwrap().subject,
        "RE: Quarterly report"
    );

    // The forward prefix keeps its own awareness too.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(original("m1", "Fwd: Quarterly report"), None, &surfaces).await;
    app.dispatch(compose(ComposeKind::Forward)).await.unwrap();
    assert_eq!(
        app.compose_request().unwrap().subject,
        "Fwd: Quarterly report"
    );

    // An empty subject prefixes to the bare word rather than a dangling space.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(original("m1", ""), None, &surfaces).await;
    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();
    assert_eq!(app.compose_request().unwrap().subject, "Re:");
}

#[tokio::test]
async fn forward_publishes_empty_recipients_the_marker_and_no_threading() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(
        original("m1", "Quarterly report"),
        Some(plain_source()),
        &surfaces,
    )
    .await;

    // A reply first, so the single slot's replacement is the thing asserted.
    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();
    app.dispatch(compose(ComposeKind::Forward)).await.unwrap();

    let snapshot = app.compose_request().expect("the forward's prefill");
    assert_eq!(snapshot.kind, ComposeKind::Forward);
    assert_eq!(snapshot.from, account("work"));
    assert_eq!(
        snapshot.to, "",
        "a forward's recipients are the user's to name"
    );
    assert_eq!(snapshot.cc, "");
    assert_eq!(snapshot.subject, "Fwd: Quarterly report");
    assert!(
        snapshot.in_reply_to.is_none(),
        "a forward answers no message, so it threads under none"
    );
    let quote = quote_of(&snapshot);
    assert_eq!(quote.attribution.line, "Forwarded message");
    // The header block a forward shows is the same a reply's is, Cc included.
    assert_eq!(quote.attribution.headers.len(), 5);
    assert!(
        quote.body_plain.contains("The numbers are in."),
        "the forward quotes the original as a reply does"
    );
    // One publish per dispatch: the second request replaced the first in the
    // one slot, and the surface said so twice.
    let signalled = surfaces
        .lock()
        .unwrap()
        .iter()
        .filter(|surface| **surface == Surface::ComposeRequest)
        .count();
    assert_eq!(signalled, 2, "every dispatch publishes its own request");
}

#[test]
fn an_html_only_original_quotes_through_sanitised_text() {
    let identity = EmailAddress::new("me@work.local");
    // The engine derives a text rendering from most HTML-only messages; the one
    // it cannot derive any text for falls back to its sanitised HTML read as
    // text, so nothing of the markup reaches the quote.
    let body = MessageBody::new(
        None,
        Some("<p>Hi <strong>there</strong></p><script>alert(1)</script>".to_owned()),
    );
    let snapshot = compose_request(
        ComposeKind::Reply,
        &account("work"),
        &original("m1", "Quarterly report"),
        &identity,
        Some(&body),
    );
    let quote = quote_of(&snapshot);
    assert_eq!(quote.body_plain, "Hi there");
    assert!(
        !quote.body_html.contains("script") && !quote.body_html.contains("alert"),
        "the sanitised text fallback carries no markup of the original: {}",
        quote.body_html
    );
}

#[test]
fn an_undated_original_attributes_the_sender_without_a_date() {
    let identity = EmailAddress::new("me@work.local");
    let mut undated = original("m1", "Quarterly report");
    undated.received_at = None;
    undated.sent_at = None;
    let body = MessageBody::new(Some("The numbers are in.".to_owned()), None);
    let snapshot = compose_request(
        ComposeKind::Reply,
        &account("work"),
        &undated,
        &identity,
        Some(&body),
    );
    let quote = quote_of(&snapshot);
    assert_eq!(
        quote.attribution.line, "Ada Lovelace <ada@example.test> wrote:",
        "no instant to name, so the sender alone is attributed"
    );
    assert!(
        !quote
            .attribution
            .headers
            .iter()
            .any(|header| header.label == "Sent"),
        "no Sent row when the original carries no instant"
    );
}

#[tokio::test]
async fn a_message_the_store_no_longer_holds_publishes_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(
        original("m1", "Quarterly report"),
        Some(plain_source()),
        &surfaces,
    )
    .await;

    let gone = crate::MessageRef::from_parts("work", "m2".to_owned()).unwrap();
    app.dispatch(Intent::Compose(ComposeIntent::Reply { message: gone }))
        .await
        .unwrap();

    assert!(
        app.compose_request().is_none(),
        "an unresolvable original opens no composer"
    );
    assert!(
        !surfaces.lock().unwrap().contains(&Surface::ComposeRequest),
        "and says nothing on the surface"
    );
}

#[tokio::test]
async fn a_bodyless_original_publishes_the_prefill_without_a_quote_block() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // No source, so the body fetch fails: the prefill still publishes, and the
    // document opens on nothing rather than an empty quote.
    let app = app_over(original("m1", "Quarterly report"), None, &surfaces).await;

    app.dispatch(compose(ComposeKind::Reply)).await.unwrap();

    let snapshot = app.compose_request().expect("the prefill still publishes");
    assert_eq!(snapshot.to, "ada@example.test");
    assert_eq!(
        snapshot.subject, "Re: Quarterly report",
        "the header prefill needs no body"
    );
    assert!(
        snapshot.document.blocks.is_empty(),
        "there is nothing to quote, so the document holds no quote block"
    );
}
