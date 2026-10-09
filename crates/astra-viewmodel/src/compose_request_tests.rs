// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the compose-request view-model: the exact JSON wire shape a reply
//! and a forward's prefill crosses the IPC in, threading header present for the
//! one and absent for the other.

use astra_composer::{Block, ComposerDocument, Quote, QuoteAttribution, QuoteHeader, QuoteStyle};

use super::{ComposeKind, ComposeRequestSnapshot};

/// The quoted original every fixture document carries, with `line`, `headers`,
/// and the two spellings of one body.
fn quote(line: &str, headers: Vec<QuoteHeader>, body: &str) -> Quote {
    Quote {
        style: QuoteStyle::Indented,
        attribution: QuoteAttribution {
            line: line.to_owned(),
            headers,
        },
        body_html: format!("<div style=\"white-space: pre-wrap;\">{body}</div>"),
        body_plain: body.to_owned(),
    }
}

/// A snapshot's document holding exactly one quote block.
fn quoted_document(block: Quote) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Quote(block)],
        attachments: Vec::new(),
    }
}

#[test]
fn a_reply_request_crosses_with_the_threading_header_and_the_quote_document() {
    let snapshot = ComposeRequestSnapshot {
        kind: ComposeKind::Reply,
        from: engine_api::AccountId::try_from("work").unwrap(),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        subject: "Re: Quarterly report".to_owned(),
        in_reply_to: Some("orig-1@server.test".to_owned()),
        document: quoted_document(quote(
            "On 9 Oct 2026 at 08:00, Ada Lovelace <ada@example.test> wrote:",
            vec![
                QuoteHeader {
                    label: "From".to_owned(),
                    value: "Ada Lovelace <ada@example.test>".to_owned(),
                },
                QuoteHeader {
                    label: "Sent".to_owned(),
                    value: "9 Oct 2026 at 08:00".to_owned(),
                },
                QuoteHeader {
                    label: "To".to_owned(),
                    value: "me@work.local, Bob <bob@example.test>".to_owned(),
                },
                QuoteHeader {
                    label: "Subject".to_owned(),
                    value: "Quarterly report".to_owned(),
                },
            ],
            "The numbers are in.",
        )),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"kind":"reply","from":"work","to":"ada@example.test","cc":"","#,
            r#""subject":"Re: Quarterly report","in_reply_to":"orig-1@server.test","#,
            r#""document":{"blocks":[{"Quote":{"style":"Indented","#,
            r#""attribution":{"line":"On 9 Oct 2026 at 08:00, Ada Lovelace "#,
            r#"<ada@example.test> wrote:","headers":[{"label":"From","value":"Ada "#,
            r#"Lovelace <ada@example.test>"},{"label":"Sent","value":"9 Oct 2026 "#,
            r#"at 08:00"},{"label":"To","value":"me@work.local, Bob "#,
            r#"<bob@example.test>"},{"label":"Subject","value":"Quarterly "#,
            r#"report"}]},"body_html":"<div style=\"white-space: "#,
            r#"pre-wrap;\">The numbers are in.</div>","body_plain":"The numbers "#,
            r#"are in."}}],"attachments":[]}}"#
        )
    );
    assert_eq!(
        serde_json::from_str::<ComposeRequestSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn a_forward_request_crosses_without_the_threading_header() {
    let snapshot = ComposeRequestSnapshot {
        kind: ComposeKind::Forward,
        from: engine_api::AccountId::try_from("work").unwrap(),
        to: String::new(),
        cc: String::new(),
        subject: "Fwd: Quarterly report".to_owned(),
        in_reply_to: None,
        document: quoted_document(quote(
            "Forwarded message",
            vec![QuoteHeader {
                label: "Subject".to_owned(),
                value: "Quarterly report".to_owned(),
            }],
            "The numbers are in.",
        )),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    // No `in_reply_to` key at all: a forward answers nothing.
    assert_eq!(
        json,
        concat!(
            r#"{"kind":"forward","from":"work","to":"","cc":"","#,
            r#""subject":"Fwd: Quarterly report","#,
            r#""document":{"blocks":[{"Quote":{"style":"Indented","#,
            r#""attribution":{"line":"Forwarded message","headers":"#,
            r#"[{"label":"Subject","value":"Quarterly report"}]},"#,
            r#""body_html":"<div style=\"white-space: pre-wrap;\">The "#,
            r#"numbers are in.</div>","body_plain":"The numbers are in."}}],"#,
            r#""attachments":[]}}"#
        )
    );
    assert_eq!(
        serde_json::from_str::<ComposeRequestSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn the_kinds_cross_as_plain_words() {
    assert_eq!(
        serde_json::to_string(&ComposeKind::Reply).unwrap(),
        r#""reply""#
    );
    assert_eq!(
        serde_json::to_string(&ComposeKind::ReplyAll).unwrap(),
        r#""reply_all""#
    );
    assert_eq!(
        serde_json::to_string(&ComposeKind::Forward).unwrap(),
        r#""forward""#
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<ComposeKind>(r#""ReplyAll""#).is_err());
}
