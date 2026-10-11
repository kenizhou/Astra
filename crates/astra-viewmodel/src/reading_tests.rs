// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading view-model: the exact JSON wire shape a snapshot of
//! the open message's body crosses the IPC in.

use engine_api::{AccountId, ProviderKey};

use super::*;
use crate::RowKey;

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
        invitation: None,
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
        invitation: None,
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

#[test]
fn a_message_row_key_becomes_a_reading_key() {
    // The tie between the two view-models: the row a list selected carries the
    // exact identity the reading pane opens, so the conversion is total for a
    // message row and loses nothing.
    let row = RowKey::Message {
        account: account("work"),
        key: key("imap:v1:u42"),
    };
    let reading = MessageKey::try_from(row).unwrap();
    assert_eq!(
        reading,
        MessageKey {
            account: account("work"),
            key: key("imap:v1:u42"),
        }
    );
    // And back again: the payload stands alone in both directions.
    assert_eq!(
        RowKey::from(reading),
        RowKey::Message {
            account: account("work"),
            key: key("imap:v1:u42"),
        }
    );
}

#[test]
fn a_thread_row_key_is_rejected_as_a_reading_key() {
    // A conversation names no single message, so the reading pane cannot open
    // one from it; the rejection is the error, not a panic or a guess at the
    // newest member.
    let row = RowKey::Thread {
        account: account("work"),
        thread_id: engine_api::ThreadId::try_from("th-7").unwrap(),
    };
    assert_eq!(MessageKey::try_from(row), Err(NotAMessageRow));
}

#[test]
fn an_invitation_block_crosses_after_the_body_with_snake_case_words() {
    let snapshot = ReadingSnapshot {
        message: MessageKey {
            account: account("work"),
            key: key("imap:v1:u42"),
        },
        subject: "Sprint planning".to_owned(),
        from_name: "Boss".to_owned(),
        from_address: "boss@test.local".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        html: None,
        plain: Some("When: 30 July".to_owned()),
        attachments: Vec::new(),
        remote_image_count: 0,
        invitation: Some(InvitationBlock {
            kind: InvitationKind::Rsvp,
            response: Participation::NeedsAction,
            stale: false,
            summary: "Sprint planning".to_owned(),
            organizer: "boss@test.local".to_owned(),
        }),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"message":{"account":"work","key":"imap:v1:u42"},"#,
            r#""subject":"Sprint planning","#,
            r#""from_name":"Boss","#,
            r#""from_address":"boss@test.local","#,
            r#""sent_at":"2026-10-09T08:30:00Z","#,
            r#""plain":"When: 30 July","#,
            r#""remote_image_count":0,"#,
            r#""invitation":{"kind":"rsvp","response":"needs_action","#,
            r#""stale":false,"summary":"Sprint planning","#,
            r#""organizer":"boss@test.local"}}"#
        )
    );

    let back: ReadingSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn every_invitation_kind_crosses_as_its_own_snake_case_word() {
    for (kind, word) in [
        (InvitationKind::Rsvp, "rsvp"),
        (InvitationKind::Cancelled, "cancelled"),
        (InvitationKind::Informational, "informational"),
        (InvitationKind::Superseded, "superseded"),
    ] {
        let block = InvitationBlock {
            kind,
            response: Participation::Tentative,
            stale: true,
            summary: "Sprint planning".to_owned(),
            organizer: "boss@test.local".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&block).unwrap(),
            format!(
                concat!(
                    r#"{{"kind":"{word}","response":"tentative","#,
                    r#""stale":true,"summary":"Sprint planning","#,
                    r#""organizer":"boss@test.local"}}"#,
                ),
                word = word
            )
        );
    }
}

#[test]
fn a_reading_snapshot_without_the_invitation_block_crosses_no_key() {
    // The additive-wire case: a snapshot from before the block existed parses
    // with no invitation, and one that holds none crosses no key at all.
    let back: ReadingSnapshot = serde_json::from_str(concat!(
        r#"{"message":{"account":"work","key":"imap:v1:u42"},"#,
        r#""subject":"S","from_name":"A","from_address":"a@b.test","#,
        r#""sent_at":"2026-10-09T08:30:00Z","remote_image_count":0}"#
    ))
    .unwrap();
    assert_eq!(back.invitation, None);

    let json = serde_json::to_string(&back).unwrap();
    assert!(!json.contains(r#""invitation""#));
}
