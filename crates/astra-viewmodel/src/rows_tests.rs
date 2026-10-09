// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the message-row types: the exact JSON wire shape each crosses the
//! IPC in, and the shape of the key a row is matched by.

use engine_api::{AccountId, ProviderKey, ThreadId};

use super::*;

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn key(value: &str) -> ProviderKey {
    ProviderKey::new(value).unwrap()
}

fn thread(value: &str) -> ThreadId {
    ThreadId::try_from(value).unwrap()
}

#[test]
fn view_mode_serialises_as_a_snake_case_word() {
    assert_eq!(serde_json::to_string(&ViewMode::Flat).unwrap(), r#""flat""#);
    assert_eq!(
        serde_json::to_string(&ViewMode::Threaded).unwrap(),
        r#""threaded""#
    );

    assert_eq!(
        serde_json::from_str::<ViewMode>(r#""flat""#).unwrap(),
        ViewMode::Flat
    );
    assert_eq!(
        serde_json::from_str::<ViewMode>(r#""threaded""#).unwrap(),
        ViewMode::Threaded
    );
    assert_eq!(ViewMode::default(), ViewMode::Flat);
}

#[test]
fn a_message_row_key_crosses_as_its_tagged_shape() {
    let message = RowKey::Message {
        account: account("work"),
        key: key("imap:v1:u42"),
    };
    assert_eq!(
        serde_json::to_string(&message).unwrap(),
        r#"{"message":{"account":"work","key":"imap:v1:u42"}}"#
    );
    assert_eq!(
        serde_json::from_str::<RowKey>(r#"{"message":{"account":"work","key":"imap:v1:u42"}}"#)
            .unwrap(),
        message
    );

    let conversation = RowKey::Thread {
        account: account("home"),
        thread_id: thread("th-7"),
    };
    assert_eq!(
        serde_json::to_string(&conversation).unwrap(),
        r#"{"thread":{"account":"home","thread_id":"th-7"}}"#
    );
    assert_eq!(
        serde_json::from_str::<RowKey>(r#"{"thread":{"account":"home","thread_id":"th-7"}}"#)
            .unwrap(),
        conversation
    );

    // The key a row is matched by: two rows differing only in account are two
    // rows, and a message key never equals a thread key.
    assert_ne!(
        message,
        RowKey::Message {
            account: account("home"),
            key: key("imap:v1:u42"),
        }
    );
    assert_ne!(
        message,
        RowKey::Thread {
            account: account("work"),
            thread_id: thread("imap:v1:u42"),
        }
    );
}

#[test]
fn a_flat_message_row_crosses_without_a_thread_count() {
    let row = MessageRowView {
        key: RowKey::Message {
            account: account("work"),
            key: key("imap:v1:u42"),
        },
        account_id: account("work"),
        sender_name: "Ada Lovelace".to_owned(),
        sender_address: "ada@example.test".to_owned(),
        subject: "Quarterly report".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        unread: true,
        flagged: false,
        has_attachments: true,
        thread_count: None,
    };

    let json = serde_json::to_string(&row).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"key":{"message":{"account":"work","key":"imap:v1:u42"}},"#,
            r#""account_id":"work","#,
            r#""sender_name":"Ada Lovelace","#,
            r#""sender_address":"ada@example.test","#,
            r#""subject":"Quarterly report","#,
            r#""sent_at":"2026-10-09T08:30:00Z","#,
            r#""unread":true,"flagged":false,"has_attachments":true}"#
        )
    );

    let back: MessageRowView = serde_json::from_str(&json).unwrap();
    assert_eq!(back, row);
}

#[test]
fn a_threaded_message_row_crosses_with_its_thread_count() {
    let row = MessageRowView {
        key: RowKey::Thread {
            account: account("home"),
            thread_id: thread("th-7"),
        },
        account_id: account("home"),
        sender_name: "Grace Hopper".to_owned(),
        sender_address: "grace@example.test".to_owned(),
        subject: "Re: Compiler design".to_owned(),
        // MC's row date: a whole-second UTC instant, empty when unknown.
        sent_at: String::new(),
        unread: false,
        flagged: true,
        has_attachments: false,
        thread_count: Some(3),
    };

    let json = serde_json::to_string(&row).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"key":{"thread":{"account":"home","thread_id":"th-7"}},"#,
            r#""account_id":"home","#,
            r#""sender_name":"Grace Hopper","#,
            r#""sender_address":"grace@example.test","#,
            r#""subject":"Re: Compiler design","#,
            r#""sent_at":"","#,
            r#""unread":false,"flagged":true,"has_attachments":false,"thread_count":3}"#
        )
    );

    let back: MessageRowView = serde_json::from_str(&json).unwrap();
    assert_eq!(back, row);
}

#[test]
fn a_row_without_a_thread_count_parses_one_with_it_absent() {
    // The same row on the wire with no `thread_count` key: an older emitter, or
    // the flat list, where the count carries no meaning.
    let back: MessageRowView = serde_json::from_str(concat!(
        r#"{"key":{"message":{"account":"work","key":"k1"}},"#,
        r#""account_id":"work","sender_name":"A","sender_address":"a@b.test","#,
        r#""subject":"S","sent_at":"2026-10-09T08:30:00Z","#,
        r#""unread":true,"flagged":false,"has_attachments":false}"#
    ))
    .unwrap();
    assert_eq!(back.thread_count, None);
}
