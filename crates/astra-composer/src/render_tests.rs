// SPDX-License-Identifier: GPL-3.0-only
//! Block rendering vectors: nested lists in both output parts, the two quote
//! styles, and the signature block with its RFC 3676 plain-text delimiter.

use crate::test_support::{aid, cid, html_doc, inline_attachment, item, text};
use crate::{
    render, Block, ComposerDocument, ComposerError, InlineContent, InlineImage, List, ListItem,
    ListKind, Paragraph, Quote, QuoteAttribution, QuoteHeader, QuoteStyle, Signature,
};

#[test]
fn renders_ordered_list_with_numbered_html_and_plain_text() {
    let document = ComposerDocument {
        blocks: vec![Block::List(List {
            kind: ListKind::Ordered,
            items: vec![item(vec![text("first")]), item(vec![text("second")])],
        })],
        attachments: Vec::new(),
    };

    let output = render(&document).expect("valid document");

    assert_eq!(
        output.html,
        html_doc("<ol><li>first</li><li>second</li></ol>")
    );
    assert_eq!(output.plain_text, "1. first\n2. second");
}

#[test]
fn renders_bullet_sublist_nested_inside_an_ordered_item() {
    let document = ComposerDocument {
        blocks: vec![Block::List(List {
            kind: ListKind::Ordered,
            items: vec![
                ListItem {
                    content: vec![text("fruits")],
                    child: Some(List {
                        kind: ListKind::Bullet,
                        items: vec![item(vec![text("apple")]), item(vec![text("pear")])],
                    }),
                },
                item(vec![text("veggies")]),
            ],
        })],
        attachments: Vec::new(),
    };

    let output = render(&document).expect("valid document");

    // The sub-list lives INSIDE its parent <li>, ordered numbering skips the nested items.
    assert_eq!(
        output.html,
        html_doc("<ol><li>fruits<ul><li>apple</li><li>pear</li></ul></li><li>veggies</li></ol>")
    );
    assert_eq!(
        output.plain_text,
        "1. fruits\n  - apple\n  - pear\n2. veggies"
    );
}

#[test]
fn renders_ordered_sublist_nested_inside_a_bullet_item() {
    let document = ComposerDocument {
        blocks: vec![Block::List(List {
            kind: ListKind::Bullet,
            items: vec![ListItem {
                content: vec![text("steps")],
                child: Some(List {
                    kind: ListKind::Ordered,
                    items: vec![item(vec![text("a")]), item(vec![text("b")])],
                }),
            }],
        })],
        attachments: Vec::new(),
    };

    let output = render(&document).expect("valid document");

    assert_eq!(
        output.html,
        html_doc("<ul><li>steps<ol><li>a</li><li>b</li></ol></li></ul>")
    );
    assert_eq!(output.plain_text, "- steps\n  1. a\n  2. b");
}

#[test]
fn nested_list_schema_round_trips_through_json() {
    let document = ComposerDocument {
        blocks: vec![Block::List(List {
            kind: ListKind::Ordered,
            items: vec![ListItem {
                content: vec![text("outer")],
                child: Some(List {
                    kind: ListKind::Bullet,
                    items: vec![item(vec![text("inner")])],
                }),
            }],
        })],
        attachments: Vec::new(),
    };

    let json = serde_json::to_string(&document).expect("serialize");
    let restored: ComposerDocument = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored, document);
}

#[test]
fn validates_inline_image_carried_inside_a_nested_list_item() {
    // The CID image lives two levels deep; if validation did not recurse, the inline
    // attachment would be reported as unused and rendering would fail.
    let document = ComposerDocument {
        blocks: vec![Block::List(List {
            kind: ListKind::Ordered,
            items: vec![ListItem {
                content: vec![text("see")],
                child: Some(List {
                    kind: ListKind::Bullet,
                    items: vec![item(vec![InlineContent::Image(InlineImage {
                        attachment_id: aid("img-1"),
                        alt_text: "chart".to_owned(),
                        width_px: None,
                    })])],
                }),
            }],
        })],
        attachments: vec![inline_attachment("img-1")],
    };

    let output = render(&document).expect("valid document");

    assert_eq!(output.inline_attachments.len(), 1);
    assert_eq!(
        output.inline_attachments[0].cid.as_ref(),
        Some(&cid("chart-1@example.test"))
    );
    assert!(output.html.contains(
        "<ol><li>see<ul><li><img src=\"cid:chart-1@example.test\" alt=\"chart\" \
         style=\"max-width: 100%\"></li></ul></li></ol>"
    ));
}

#[test]
fn rejects_missing_inline_image_inside_a_nested_list_item() {
    let document = ComposerDocument {
        blocks: vec![Block::List(List {
            kind: ListKind::Bullet,
            items: vec![ListItem {
                content: vec![text("outer")],
                child: Some(List {
                    kind: ListKind::Bullet,
                    items: vec![item(vec![InlineContent::Image(InlineImage {
                        attachment_id: aid("missing"),
                        alt_text: String::new(),
                        width_px: None,
                    })])],
                }),
            }],
        })],
        attachments: Vec::new(),
    };

    assert!(matches!(
        render(&document),
        Err(ComposerError::MissingInlineAttachment { .. })
    ));
}

fn quote_headers() -> Vec<QuoteHeader> {
    vec![
        QuoteHeader {
            label: "From".to_owned(),
            value: "Alice <alice@x.test>".to_owned(),
        },
        QuoteHeader {
            label: "Sent".to_owned(),
            value: "30 Jun 2026 14:03".to_owned(),
        },
        QuoteHeader {
            label: "To".to_owned(),
            value: "me@x.test".to_owned(),
        },
        QuoteHeader {
            label: "Subject".to_owned(),
            value: "Lunch".to_owned(),
        },
    ]
}

fn quote_doc(style: QuoteStyle) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![
            Block::Paragraph(Paragraph {
                content: vec![text("Sounds good.")],
            }),
            Block::Quote(Quote {
                style,
                attribution: QuoteAttribution {
                    // Carries `<`/`>` so escaping in the attribution context is covered.
                    line: "On 30 Jun 2026 14:03, Alice <alice@x.test> wrote:".to_owned(),
                    headers: quote_headers(),
                },
                body_html: "<p>Are we still on for <strong>lunch</strong>?</p>".to_owned(),
                body_plain: "Are we still on for lunch?".to_owned(),
            }),
        ],
        attachments: Vec::new(),
    }
}

#[test]
fn renders_the_indented_style_as_a_left_bordered_blockquote() {
    let output = render(&quote_doc(QuoteStyle::Indented)).expect("valid document");

    assert_eq!(
        output.html,
        html_doc(
            "<p>Sounds good.</p>\
             <p>On 30 Jun 2026 14:03, Alice &lt;alice@x.test&gt; wrote:</p>\
             <blockquote style=\"margin:0 0 0 0.8ex;border-left:2px solid #cccccc;padding-left:1ex\">\
             <p>Are we still on for <strong>lunch</strong>?</p></blockquote>"
        )
    );
    // The attribution leads, then every quoted line is `> `-prefixed.
    assert_eq!(
        output.plain_text,
        "Sounds good.\nOn 30 Jun 2026 14:03, Alice <alice@x.test> wrote:\n> Are we still on for lunch?"
    );
}

#[test]
fn renders_the_line_and_header_style_with_a_top_border_divider_and_header_block() {
    let output = render(&quote_doc(QuoteStyle::LineAndHeader)).expect("valid document");

    // The divider is a top border in the interop blue-grey (not an <hr>), and labels are
    // bold-with-colon-and-space (`<b>From: </b>value`); what an Outlook reader expects.
    assert_eq!(
        output.html,
        html_doc(
            "<p>Sounds good.</p>\
             <div style=\"padding:3pt 0 0;border-top:1pt solid rgb(181, 196, 223)\"><div>\
             <strong>From: </strong>Alice &lt;alice@x.test&gt;<br>\
             <strong>Sent: </strong>30 Jun 2026 14:03<br>\
             <strong>To: </strong>me@x.test<br>\
             <strong>Subject: </strong>Lunch<br>\
             </div></div><div><p>Are we still on for <strong>lunch</strong>?</p></div>"
        )
    );
    assert_eq!(
        output.plain_text,
        "Sounds good.\n________________________________\n\
         From: Alice <alice@x.test>\nSent: 30 Jun 2026 14:03\nTo: me@x.test\n\
         Subject: Lunch\nAre we still on for lunch?"
    );
}

#[test]
fn quote_body_html_is_emitted_verbatim_not_re_escaped() {
    // The body fragment is the core's pre-sanitised HTML; the composer must not escape it
    // (that would double-encode the original); it appears byte-for-byte in the output.
    let output = render(&quote_doc(QuoteStyle::Indented)).expect("valid document");
    assert!(output
        .html
        .contains("<p>Are we still on for <strong>lunch</strong>?</p>"));
}

#[test]
fn quote_block_round_trips_through_serde() {
    let document = quote_doc(QuoteStyle::LineAndHeader);
    let json = serde_json::to_string(&document).expect("serialize");
    let back: ComposerDocument = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(document, back);
}

#[test]
fn the_style_serializes_under_its_variant_name_the_editor_and_clients_hardcode() {
    // These two tokens are a cross-language contract, not an implementation detail: the shared
    // editor page switches on `data-quote-style`, and each client's QuoteSeed builder emits
    // the same string into the seed JSON. Renaming a variant silently changes the wire token
    // and would leave every client falling back to the default style.
    let json = serde_json::to_string(&QuoteStyle::Indented).expect("serialize");
    assert_eq!(json, "\"Indented\"");
    let json = serde_json::to_string(&QuoteStyle::LineAndHeader).expect("serialize");
    assert_eq!(json, "\"LineAndHeader\"");
}

fn signature_doc() -> ComposerDocument {
    ComposerDocument {
        blocks: vec![
            Block::Paragraph(Paragraph {
                content: vec![text("See you Thursday.")],
            }),
            Block::Signature(Signature {
                body_html: "<p>Alice Doe<br><em>Astra</em></p>".to_owned(),
                body_plain: "Alice Doe\nAstra".to_owned(),
            }),
        ],
        attachments: Vec::new(),
    }
}

#[test]
fn renders_the_signature_inside_its_wrapper_with_the_body_verbatim() {
    let output = render(&signature_doc()).expect("valid document");

    // The fragment is the user's authored HTML: emitted byte-for-byte, never re-escaped (that
    // would show them the tags they wrote). The wrapper matches the editor's region class.
    assert_eq!(
        output.html,
        html_doc(
            "<p>See you Thursday.</p>\
             <div class=\"allodia-signature\">\
             <p>Alice Doe<br><em>Astra</em></p></div>"
        )
    );
}

#[test]
fn the_plain_text_signature_opens_with_the_rfc_3676_delimiter_including_its_trailing_space() {
    let output = render(&signature_doc()).expect("valid document");

    // "-- " (with the trailing space) is what a reader and a mailing list key trailing-signature
    // detection off, asserting the whole string pins the space a trim would silently eat.
    assert_eq!(
        output.plain_text,
        "See you Thursday.\n-- \nAlice Doe\nAstra"
    );
}

#[test]
fn a_signature_with_no_plain_text_still_emits_the_delimiter() {
    // An images-only signature has nothing to say in text/plain, but the delimiter still marks
    // where the message ended; otherwise the text part would just stop with no boundary.
    let document = ComposerDocument {
        blocks: vec![Block::Signature(Signature {
            body_html: "<img src=\"data:image/png;base64,iVBORw0KGgo=\" alt=\"logo\">".to_owned(),
            body_plain: String::new(),
        })],
        attachments: Vec::new(),
    };

    let output = render(&document).expect("valid document");
    assert_eq!(output.plain_text, "-- ");
    assert!(output.html.contains("data:image/png;base64,iVBORw0KGgo="));
}

#[test]
fn a_signature_sits_above_the_quoted_original_when_the_editor_put_it_there() {
    // Outlook's placement: the reply text, then the signature, then the quote. Block order is
    // the editor's to decide; the renderer must not reorder it (a signature emitted *below* the
    // quote would read as part of the original message).
    let document = ComposerDocument {
        blocks: vec![
            Block::Paragraph(Paragraph {
                content: vec![text("Sounds good.")],
            }),
            Block::Signature(Signature {
                body_html: "<p>Alice</p>".to_owned(),
                body_plain: "Alice".to_owned(),
            }),
            Block::Quote(Quote {
                style: QuoteStyle::Indented,
                attribution: QuoteAttribution {
                    line: "On 30 Jun 2026, Bob wrote:".to_owned(),
                    headers: Vec::new(),
                },
                body_html: "<p>Lunch?</p>".to_owned(),
                body_plain: "Lunch?".to_owned(),
            }),
        ],
        attachments: Vec::new(),
    };

    let output = render(&document).expect("valid document");
    let signature_at = output.html.find("allodia-signature").expect("signature");
    let quote_at = output.html.find("blockquote").expect("quote");
    assert!(signature_at < quote_at);
    assert_eq!(
        output.plain_text,
        "Sounds good.\n-- \nAlice\nOn 30 Jun 2026, Bob wrote:\n> Lunch?"
    );
}

#[test]
fn signature_block_round_trips_through_serde() {
    let document = signature_doc();
    let json = serde_json::to_string(&document).expect("serialize");
    let back: ComposerDocument = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(document, back);
}

#[test]
fn the_signature_block_serializes_under_the_tag_the_editor_emits() {
    // A cross-language contract, like `QuoteStyle`'s variant names: the shared editor page
    // emits `{ "Signature": { … } }` from its signature region. Renaming the variant would
    // silently change the wire tag and every client's signature would arrive as an
    // unparseable block.
    let json = serde_json::to_string(&Block::Signature(Signature {
        body_html: "<p>x</p>".to_owned(),
        body_plain: "x".to_owned(),
    }))
    .expect("serialize");
    assert_eq!(
        json,
        "{\"Signature\":{\"body_html\":\"<p>x</p>\",\"body_plain\":\"x\"}}"
    );
}
