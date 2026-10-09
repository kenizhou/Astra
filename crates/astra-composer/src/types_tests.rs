// SPDX-License-Identifier: GPL-3.0-only
//! Document-level vectors: marks, lists, and tables in both output parts, the
//! table and content-id validation edges, the width clamp at ingestion, and the
//! JSON round trip of the whole document schema.

use crate::test_support::{html_doc, item, regular_attachment, rich_text, text};
use crate::{
    render, Block, ComposerDocument, ComposerError, ContentId, InlineContent, List, ListKind,
    Paragraph, Table, TableCell, TableRow,
};

/// The inline cell style emitted for every table cell.
const CELL: &str = " style=\"border:1px solid #d8dde3;padding:6px;vertical-align:top\"";

#[test]
fn renders_marks_lists_tables_and_html_escapes_text() {
    let document = ComposerDocument {
        blocks: vec![
            Block::Paragraph(Paragraph {
                content: vec![text("Hello "), rich_text("<world>")],
            }),
            Block::List(List {
                kind: ListKind::Bullet,
                items: vec![item(vec![text("one")]), item(vec![text("two")])],
            }),
            Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![
                            TableCell {
                                content: vec![text("A1")],
                            },
                            TableCell {
                                content: vec![text("B1")],
                            },
                        ],
                    },
                    TableRow {
                        cells: vec![
                            TableCell {
                                content: vec![text("A2")],
                            },
                            TableCell {
                                content: vec![text("B2")],
                            },
                        ],
                    },
                ],
            }),
        ],
        attachments: Vec::new(),
    };

    let output = render(&document).expect("valid document");

    assert_eq!(
        output.html,
        html_doc(&format!(
            "<p>Hello <span style=\"font-size:18px\"><strong><em><u>&lt;world&gt;</u></em></strong></span></p>\
             <ul><li>one</li><li>two</li></ul>\
             <table style=\"border-collapse:collapse\">\
             <tr><td{CELL}>A1</td><td{CELL}>B1</td></tr>\
             <tr><td{CELL}>A2</td><td{CELL}>B2</td></tr></table>"
        ))
    );
    assert_eq!(
        output.plain_text,
        "Hello <world>\n- one\n- two\nA1 | B1\nA2 | B2"
    );
    assert!(output.inline_attachments.is_empty());
    assert!(output.attachments.is_empty());
}

#[test]
fn rejects_ragged_tables() {
    let document = ComposerDocument {
        blocks: vec![Block::Table(Table {
            rows: vec![
                TableRow {
                    cells: vec![TableCell {
                        content: vec![text("one")],
                    }],
                },
                TableRow {
                    cells: vec![
                        TableCell {
                            content: vec![text("two")],
                        },
                        TableCell {
                            content: vec![text("three")],
                        },
                    ],
                },
            ],
        })],
        attachments: Vec::new(),
    };

    assert!(matches!(
        render(&document),
        Err(ComposerError::RaggedTable {
            row: 1,
            expected: 1,
            actual: 2
        })
    ));
}

#[test]
fn content_id_rejects_angle_brackets_like_the_engine_header() {
    assert!(ContentId::new("a<b").is_none());
    assert!(ContentId::new("a>b").is_none());
    assert!(ContentId::new("chart-1@example.test").is_some());
}

#[test]
fn over_large_width_px_clamps_to_u16_max_at_ingestion() {
    let json = r#"{
        "blocks": [
            {
                "Paragraph": {
                    "content": [
                        {
                            "Image": {
                                "attachment_id": "img-1",
                                "alt_text": "chart",
                                "width_px": 70000
                            }
                        }
                    ]
                }
            }
        ],
        "attachments": []
    }"#;

    let document: ComposerDocument = serde_json::from_str(json).expect("clamped parse");
    let Block::Paragraph(paragraph) = &document.blocks[0] else {
        panic!("expected paragraph");
    };
    let InlineContent::Image(image) = &paragraph.content[0] else {
        panic!("expected image");
    };
    assert_eq!(image.width_px, Some(u16::MAX));
}

#[test]
fn document_schema_round_trips_through_json() {
    let document = ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![text("hello")],
        })],
        attachments: vec![regular_attachment("file-1")],
    };

    let json = serde_json::to_string(&document).expect("serialize");
    let restored: ComposerDocument = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored, document);
}
