// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the agent draft seam: the prefill an assistant's draft opens
//! (the agent's own recipients and words, not the original's), the threading
//! and quote a `reply_to` adds, the unthreaded fallback when the original
//! has moved, the refusal when no account exists, and the pin that nothing
//! sends. The fixtures live in `dispatch_fakes`, shared with the compose
//! family's own tests.

use std::sync::{Arc, Mutex};

use astra_composer::Block;
use engine_api::{AccountId, EmailAddress, Message, MessageIdHeader};

use crate::{dispatch_fakes as fakes, reference::MessageRef, OpenDraftError, Surface};

/// A well-formed account id, the way the tests name one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// The original an agent draft answers: Ada writing to the account's own
/// identity, under her own message-id.
fn original() -> Message {
    let mut message = fakes::mail("m1", "inbox", "Quarterly report");
    message.envelope.to = vec![EmailAddress::new("me@work.local")];
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

/// An app over one `work` account (identity `me@work.local`) whose Inbox
/// holds the original and its source, synced so the store resolves it.
async fn app_over(surfaces: &Arc<Mutex<Vec<Surface>>>) -> crate::App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![original()],
        vec![("m1".to_owned(), plain_source())],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    app
}

/// The reference to the original.
fn m1() -> MessageRef {
    MessageRef::from_parts("work", "m1".to_owned()).unwrap()
}

/// The joined text of the document's leading paragraph blocks, in order.
fn paragraph_texts(snapshot: &astra_viewmodel::ComposeRequestSnapshot) -> Vec<String> {
    snapshot
        .document
        .blocks
        .iter()
        .map_while(|block| match block {
            Block::Paragraph(paragraph) => Some(
                paragraph
                    .content
                    .iter()
                    .map(|inline| match inline {
                        astra_composer::InlineContent::Text(run) => run.text.clone(),
                        astra_composer::InlineContent::Image(_) => String::new(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_agent_draft_opens_on_the_compose_request_surface() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(&surfaces).await;

    app.compose_agent_draft(
        None,
        &["ada@example.test".to_owned(), "  ".to_owned()],
        &["carol@example.test".to_owned()],
        "The report",
        "First line.\n\nSecond line.",
        None,
    )
    .await
    .expect("the draft opens");

    let snapshot = app.compose_request().expect("the compose request");
    assert_eq!(
        snapshot.from,
        account("work"),
        "no account named: the first"
    );
    assert_eq!(snapshot.to, "ada@example.test", "blank recipients dropped");
    assert_eq!(snapshot.cc, "carol@example.test");
    assert_eq!(snapshot.subject, "The report");
    assert_eq!(
        snapshot.in_reply_to, None,
        "a draft with nothing to answer threads under nothing"
    );
    assert_eq!(
        paragraph_texts(&snapshot),
        ["First line.", "", "Second line."],
        "the agent's words are the document, one paragraph per line"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::ComposeRequest),
        "the surface the composer hydrates from is signalled"
    );
}

#[tokio::test]
async fn an_agent_reply_threads_under_the_original_and_quotes_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(&surfaces).await;
    let message = m1();

    app.compose_agent_draft(
        Some(&account("work")),
        &["ada@example.test".to_owned()],
        &[],
        "Re: Quarterly report",
        "Agreed.",
        Some(&message),
    )
    .await
    .expect("the draft opens");

    let snapshot = app.compose_request().expect("the compose request");
    assert_eq!(snapshot.in_reply_to.as_deref(), Some("orig-1@server.test"));
    assert_eq!(paragraph_texts(&snapshot), ["Agreed."]);
    match snapshot.document.blocks.as_slice() {
        [Block::Paragraph(_), Block::Quote(quote)] => {
            assert!(
                quote.body_plain.contains("The numbers are in.")
                    && quote.body_plain.contains("Second line."),
                "the quoted original below the agent's words: {}",
                quote.body_plain
            );
        }
        blocks => panic!("the agent's words above one quote, not {blocks:?}"),
    }
}

#[tokio::test]
async fn an_agent_reply_whose_original_moved_opens_unthreaded() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![fakes::inbox()], Vec::new());
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    let gone = MessageRef::from_parts("work", "gone".to_owned()).unwrap();

    app.compose_agent_draft(
        None,
        &["ada@example.test".to_owned()],
        &[],
        "Re: nothing",
        "Words.",
        Some(&gone),
    )
    .await
    .expect("the draft still opens");

    let snapshot = app.compose_request().expect("the compose request");
    assert_eq!(snapshot.in_reply_to, None, "nothing to thread under");
    assert_eq!(paragraph_texts(&snapshot), ["Words."]);
}

#[tokio::test]
async fn an_agent_draft_without_any_account_is_refused() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(Vec::new(), &surfaces);

    let refused = app
        .compose_agent_draft(None, &[], &[], "S", "B", None)
        .await
        .unwrap_err();

    assert_eq!(refused, OpenDraftError::NoAccount);
    assert!(
        app.compose_request().is_none(),
        "nothing publishes when no account can compose"
    );
}

/// The seam's own never-send pin: a draft publishes a prefill and drives no
/// submit, no outbox op, no provider delivery.
#[tokio::test]
async fn opening_an_agent_draft_sends_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![original()],
        vec![("m1".to_owned(), plain_source())],
    );
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    let message = m1();

    app.compose_agent_draft(
        None,
        &["ada@example.test".to_owned()],
        &[],
        "Re: Quarterly report",
        "Agreed.",
        Some(&message),
    )
    .await
    .expect("the draft opens");

    assert!(
        submits.lock().unwrap().is_empty(),
        "nothing leaves the device until the human presses send"
    );
}
