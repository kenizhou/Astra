// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading view-model: the exact JSON wire shape a snapshot of
//! the open message's body crosses the IPC in.

use engine_api::{AccountId, ProviderKey};

use super::*;

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn key(value: &str) -> ProviderKey {
    ProviderKey::new(value).unwrap()
}

#[test]
fn a_full_reading_snapshot_crosses_with_every_optional_key() {
    let snapshot = ReadingSnapshot {
        message: MessageKey {
            account: account("work"),
            key: key("imap:v1:u42"),
        },
        subject: "Quarterly report".to_owned(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        html: Some("<p>See the report.</p>".to_owned()),
        plain: Some("See the report.".to_owned()),
        attachments: vec![
            AttachmentRow {
                id: 1,
                file_name: "report.pdf".to_owned(),
                media_type: "application/pdf".to_owned(),
                size: 2048,
            },
            AttachmentRow {
                id: 2,
                file_name: "chart.png".to_owned(),
                media_type: "image/png".to_owned(),
                size: 512,
            },
        ],
        remote_image_count: 2,
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"message":{"account":"work","key":"imap:v1:u42"},"#,
            r#""subject":"Quarterly report","#,
            r#""from_name":"Ada Lovelace","#,
            r#""from_address":"ada@example.test","#,
            r#""sent_at":"2026-10-09T08:30:00Z","#,
            r#""html":"<p>See the report.</p>","#,
            r#""plain":"See the report.","#,
            r#""attachments":["#,
            r#"{"id":1,"file_name":"report.pdf","media_type":"application/pdf","size":2048},"#,
            r#"{"id":2,"file_name":"chart.png","media_type":"image/png","size":512}],"#,
            r#""remote_image_count":2}"#
        )
    );

    let back: ReadingSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn a_plain_only_reading_snapshot_crosses_with_no_optional_keys() {
    let snapshot = ReadingSnapshot {
        message: MessageKey {
            account: account("home"),
            key: key("imap:v1:u7"),
        },
        subject: "Lunch".to_owned(),
        from_name: "Grace Hopper".to_owned(),
        from_address: "grace@example.test".to_owned(),
        sent_at: "2026-10-09T09:15:00Z".to_owned(),
        html: None,
        plain: Some("Sandwiches at noon.".to_owned()),
        attachments: Vec::new(),
        remote_image_count: 0,
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"message":{"account":"home","key":"imap:v1:u7"},"#,
            r#""subject":"Lunch","#,
            r#""from_name":"Grace Hopper","#,
            r#""from_address":"grace@example.test","#,
            r#""sent_at":"2026-10-09T09:15:00Z","#,
            r#""plain":"Sandwiches at noon.","#,
            r#""remote_image_count":0}"#
        )
    );
    // The at-rest optionals are absent from the wire, not carried as nulls or
    // empty collections: no html, no attachments key at all.
    assert!(!json.contains(r#""html""#));
    assert!(!json.contains(r#""attachments""#));

    let back: ReadingSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn a_bodyless_reading_snapshot_parses_without_optional_keys() {
    // The same header-only shape on the wire with neither body key: a message
    // that carries no text body, or an emitter that never writes the keys.
    let back: ReadingSnapshot = serde_json::from_str(concat!(
        r#"{"message":{"account":"work","key":"imap:v1:u42"},"#,
        r#""subject":"S","from_name":"A","from_address":"a@b.test","#,
        r#""sent_at":"2026-10-09T08:30:00Z","remote_image_count":0}"#
    ))
    .unwrap();
    assert_eq!(back.html, None);
    assert_eq!(back.plain, None);
    assert!(back.attachments.is_empty());
}

#[test]
fn a_reading_key_is_the_account_and_message_pair() {
    // The key a snapshot is matched by: the same provider key in two accounts
    // names two messages, and only the pair identifies the open one.
    let first = MessageKey {
        account: account("work"),
        key: key("imap:v1:u42"),
    };
    let second = MessageKey {
        account: account("home"),
        key: key("imap:v1:u42"),
    };
    assert_ne!(first, second);
    assert_eq!(
        serde_json::to_string(&second).unwrap(),
        r#"{"account":"home","key":"imap:v1:u42"}"#
    );
}
