// SPDX-License-Identifier: GPL-3.0-only
//! Wire tests for the submit family's threaded arms, split from
//! `protocol_tests.rs` so that file stays under the 500-line cap. The plain
//! submit lives there; the reply and forward cross here, with the threading
//! header the reply carries round trip and the blank a wire reader must
//! refuse.

use astra_composer::{Block, ComposerDocument, DraftBlobHandle, InlineContent, Paragraph, TextRun};
use engine_api::{AccountId, MessageIdHeader};

use super::{ComposerBlob, Intent};

/// A well-formed account id, the way every test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// The original's `Message-ID`, the way the threaded tests name one.
fn header(value: &str) -> MessageIdHeader {
    MessageIdHeader::new(value).unwrap()
}

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

/// A one-paragraph document carrying `text`, the smallest document a send holds.
fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

#[test]
fn submit_rich_reply_carries_the_threading_header_beside_the_document() {
    round_trips(
        &Intent::SubmitRichReply {
            from: account("work"),
            to: "ada@example.test".to_owned(),
            cc: String::new(),
            in_reply_to: header("orig-1@server.test"),
            subject: "Re: Quarterly report".to_owned(),
            document: document("Agreed"),
            blobs: Vec::new(),
            crypto: crate::crypto::CryptoFlags::default(),
        },
        concat!(
            r#"{"submit_rich_reply":{"from":"work","to":"ada@example.test","#,
            r#""cc":"","in_reply_to":"orig-1@server.test","#,
            r#""subject":"Re: Quarterly report","#,
            r#""document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"Agreed","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"blobs":[],"crypto":{}}}"#
        ),
    );
    // A reply with host-resolved attachment bytes carries them beside the
    // document, exactly as the plain submit does.
    round_trips(
        &Intent::SubmitRichReply {
            from: account("work"),
            to: "ada@example.test".to_owned(),
            cc: "carol@example.test".to_owned(),
            in_reply_to: header("orig-1@server.test"),
            subject: String::new(),
            document: document("With a file"),
            blobs: vec![ComposerBlob::new(
                DraftBlobHandle::new("blob-1").unwrap(),
                vec![1, 2, 3],
            )],
            crypto: crate::crypto::CryptoFlags::default(),
        },
        concat!(
            r#"{"submit_rich_reply":{"from":"work","to":"ada@example.test","#,
            r#""cc":"carol@example.test","in_reply_to":"orig-1@server.test","#,
            r#""subject":"","document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"With a file","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"blobs":[{"handle":"blob-1","bytes":[1,2,3]}],"crypto":{}}}"#
        ),
    );
}

#[test]
fn submit_rich_forward_carries_no_threading_header() {
    round_trips(
        &Intent::SubmitRichForward {
            from: account("work"),
            to: "bob@example.test".to_owned(),
            subject: "Fwd: Quarterly report".to_owned(),
            document: document("Look at this"),
            blobs: Vec::new(),
            crypto: crate::crypto::CryptoFlags::default(),
        },
        concat!(
            r#"{"submit_rich_forward":{"from":"work","to":"bob@example.test","#,
            r#""subject":"Fwd: Quarterly report","#,
            r#""document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"Look at this","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"blobs":[],"crypto":{}}}"#
        ),
    );
}

#[test]
fn a_blank_threading_header_is_refused_on_the_wire() {
    // The header newtype validates on the way in, so a blank cannot ride a
    // reply into the core and name an original no message carries.
    let blank = concat!(
        r#"{"submit_rich_reply":{"from":"work","to":"ada@example.test","#,
        r#""cc":"","in_reply_to":"","subject":"Re:","#,
        r#""document":{"blocks":[],"attachments":[]},"blobs":[],"crypto":{}}}"#
    );
    assert!(serde_json::from_str::<Intent>(blank).is_err());
    assert_eq!(
        header("orig-1@server.test").as_str(),
        "orig-1@server.test",
        "the header reads back as the host gave it"
    );
}
