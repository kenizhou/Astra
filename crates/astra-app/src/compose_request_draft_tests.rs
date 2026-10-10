// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the accepted-chip arm: the compose-from-draft publish (the
//! reply's own prefill with the drafted reply already the document's opening
//! paragraphs, signalled through the compose-request surface the composer
//! hydrates from) and the draft document's own shape (one paragraph per
//! line, blank lines kept, the quote below, nothing quoted when the original
//! carried no body). A child of the compose tests, sharing their fixtures.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, InlineContent, Paragraph, TextRun};
use astra_viewmodel::ComposeKind;
use engine_api::{EmailAddress, MessageBody, ProviderKey};

use super::{account, app_over, original, paragraph_texts, plain_source};
use crate::{compose_request::compose_request, Surface};

#[tokio::test]
async fn an_accepted_draft_publishes_the_reply_prefill_with_the_draft_as_the_document() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(
        original("m1", "Quarterly report"),
        Some(plain_source()),
        &surfaces,
    )
    .await;

    app.compose_from_draft(
        &account("work"),
        &ProviderKey::new("m1").expect("a provider key"),
        "Sounds good.\nShipped it.",
    )
    .await;

    let snapshot = app.compose_request().expect("the accepted draft's prefill");
    assert_eq!(
        snapshot.kind,
        ComposeKind::Reply,
        "a chip answers the sender"
    );
    assert_eq!(snapshot.to, "ada@example.test");
    assert_eq!(snapshot.subject, "Re: Quarterly report");
    assert_eq!(snapshot.in_reply_to.as_deref(), Some("orig-1@server.test"));
    assert_eq!(
        paragraph_texts(&snapshot),
        vec!["Sounds good.".to_owned(), "Shipped it.".to_owned()],
        "the drafted reply rides the document as its opening paragraphs"
    );
    assert!(
        matches!(
            snapshot.document.blocks.as_slice(),
            [Block::Paragraph(_), Block::Paragraph(_), Block::Quote(_)]
        ),
        "the quoted original stays below the draft, the editor's own layout"
    );
    assert!(
        quote_of_lines(&snapshot).contains("The numbers are in."),
        "the quote below the draft is the original's body"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::ComposeRequest),
        "the accept publishes through the compose-request surface, so the composer opens on it"
    );
}

#[test]
fn a_draft_opens_one_paragraph_per_line_with_blank_lines_kept() {
    let identity = EmailAddress::new("me@work.local");
    let body = MessageBody::new(Some("The numbers are in.".to_owned()), None);
    let snapshot = compose_request(
        ComposeKind::Reply,
        &account("work"),
        &original("m1", "Quarterly report"),
        &identity,
        Some(&body),
        Some("Thanks!\n\nShipped it."),
    );
    assert_eq!(
        paragraph_texts(&snapshot),
        vec!["Thanks!".to_owned(), String::new(), "Shipped it.".to_owned()],
        "one paragraph per line, a blank line an empty paragraph: the plain-text resume path's own convention"
    );
    assert_eq!(
        snapshot.document.blocks.first(),
        Some(&Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: "Thanks!".to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })),
        "the draft's paragraph is one unmarked run, the shape the editor hydrates"
    );
    assert!(
        matches!(snapshot.document.blocks.last(), Some(Block::Quote(_))),
        "the quote still closes the document below the draft's paragraphs"
    );
}

#[test]
fn a_draft_with_nothing_to_quote_opens_on_the_draft_paragraphs_alone() {
    let identity = EmailAddress::new("me@work.local");
    let snapshot = compose_request(
        ComposeKind::Reply,
        &account("work"),
        &original("m1", "Quarterly report"),
        &identity,
        None,
        Some("Thanks!"),
    );
    assert_eq!(paragraph_texts(&snapshot), vec!["Thanks!".to_owned()]);
    assert_eq!(
        snapshot.document.blocks.len(),
        1,
        "no body to quote, so the draft's own paragraph is the whole document"
    );
}

/// The draft-prefilled document's quoted original, the plain text the quote
/// block below the paragraphs carries.
fn quote_of_lines(snapshot: &astra_viewmodel::ComposeRequestSnapshot) -> &str {
    match snapshot.document.blocks.last() {
        Some(Block::Quote(quote)) => &quote.body_plain,
        other => panic!("a quote closes the document, not {other:?}"),
    }
}
