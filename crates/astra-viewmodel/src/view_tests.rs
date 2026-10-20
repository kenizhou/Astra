// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the mailbox-list snapshot: the exact JSON wire shape it crosses the IPC in,
//! and that it carries every account's folder tree intact.

use engine_api::{AccountId, MailboxId, ProviderKey, ThreadId};

use super::*;
use crate::folders::{FolderRole, FolderRow};

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn folder(value: &str) -> MailboxId {
    MailboxId::try_from(value).unwrap()
}

fn message_key(value: &str) -> ProviderKey {
    ProviderKey::new(value).unwrap()
}

/// A one-folder account tree: the smallest row set that still exercises every field.
fn inbox_row(unread: u32) -> FolderRow {
    FolderRow {
        key: folder("INBOX"),
        name: "Inbox".to_owned(),
        role: Some(FolderRole::Inbox),
        unread,
        parent: None,
        depth: 0,
        has_children: false,
        expanded: false,
    }
}

#[test]
fn a_snapshot_round_trips_through_json_with_the_exact_wire_shape() {
    let snapshot = MailboxListSnapshot {
        account_folders: vec![
            AccountFolderRow {
                account_id: account("work"),
                display_name: "me@work.example".to_owned(),
                expanded: true,
                can_edit_folders: false,
                folders: vec![
                    FolderRow {
                        key: folder("INBOX"),
                        name: "Inbox".to_owned(),
                        role: Some(FolderRole::Inbox),
                        unread: 545,
                        parent: None,
                        depth: 0,
                        has_children: false,
                        expanded: false,
                    },
                    FolderRow {
                        key: folder("Clients"),
                        name: "Clients".to_owned(),
                        role: None,
                        unread: 0,
                        parent: None,
                        depth: 0,
                        has_children: true,
                        expanded: true,
                    },
                    FolderRow {
                        key: folder("Clients/Acme"),
                        name: "Acme".to_owned(),
                        role: None,
                        unread: 0,
                        parent: Some(folder("Clients")),
                        depth: 1,
                        has_children: false,
                        expanded: false,
                    },
                ],
            },
            AccountFolderRow {
                account_id: account("home"),
                display_name: "Home".to_owned(),
                expanded: false,
                can_edit_folders: false,
                folders: vec![],
            },
        ],
        ..Default::default()
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"account_folders":["#,
            r#"{"account_id":"work","display_name":"me@work.example","expanded":true,"folders":["#,
            r#"{"key":"INBOX","name":"Inbox","role":"inbox","unread":545,"depth":0,"has_children":false,"expanded":false},"#,
            r#"{"key":"Clients","name":"Clients","unread":0,"depth":0,"has_children":true,"expanded":true},"#,
            r#"{"key":"Clients/Acme","name":"Acme","unread":0,"parent":"Clients","depth":1,"has_children":false,"expanded":false}]},"#,
            r#"{"account_id":"home","display_name":"Home","expanded":false,"folders":[]}]}"#
        )
    );

    let back: MailboxListSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn an_empty_snapshot_crosses_as_an_empty_account_list() {
    let json = serde_json::to_string(&MailboxListSnapshot::default()).unwrap();
    assert_eq!(json, r#"{"account_folders":[]}"#);

    let back: MailboxListSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, MailboxListSnapshot::default());
}

#[test]
fn a_snapshot_with_message_rows_crosses_with_the_list_fields_after_the_trees() {
    let message_row = MessageRowView {
        key: RowKey::Message {
            account: account("work"),
            key: message_key("imap:v1:u42"),
        },
        account_id: account("work"),
        sender_name: "Ada Lovelace".to_owned(),
        sender_address: "ada@example.test".to_owned(),
        subject: "Quarterly report".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        unread: true,
        flagged: false,
        has_attachments: true,
        // A flat row carries no thread count; only a threaded one does.
        thread_count: None,
    };
    let thread_row = MessageRowView {
        key: RowKey::Thread {
            account: account("work"),
            thread_id: ThreadId::try_from("th-7").unwrap(),
        },
        account_id: account("work"),
        sender_name: "Grace Hopper".to_owned(),
        sender_address: "grace@example.test".to_owned(),
        subject: "Re: Compiler design".to_owned(),
        sent_at: "2026-10-08T16:05:00Z".to_owned(),
        unread: false,
        flagged: true,
        has_attachments: false,
        thread_count: Some(3),
    };
    let snapshot = MailboxListSnapshot {
        account_folders: vec![AccountFolderRow {
            account_id: account("work"),
            display_name: "me@work.example".to_owned(),
            expanded: true,
            can_edit_folders: false,
            folders: vec![inbox_row(0)],
        }],
        rows: vec![thread_row, message_row],
        selection: Some(RowKey::Thread {
            account: account("work"),
            thread_id: ThreadId::try_from("th-7").unwrap(),
        }),
        visible_limit: 100,
        view_mode: ViewMode::Threaded,
        search: None,
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"account_folders":[{"account_id":"work","display_name":"me@work.example","expanded":true,"folders":["#,
            r#"{"key":"INBOX","name":"Inbox","role":"inbox","unread":0,"depth":0,"has_children":false,"expanded":false}]}],"#,
            r#""rows":["#,
            r#"{"key":{"thread":{"account":"work","thread_id":"th-7"}},"account_id":"work","sender_name":"Grace Hopper","sender_address":"grace@example.test","subject":"Re: Compiler design","sent_at":"2026-10-08T16:05:00Z","unread":false,"flagged":true,"has_attachments":false,"thread_count":3},"#,
            r#"{"key":{"message":{"account":"work","key":"imap:v1:u42"}},"account_id":"work","sender_name":"Ada Lovelace","sender_address":"ada@example.test","subject":"Quarterly report","sent_at":"2026-10-09T08:30:00Z","unread":true,"flagged":false,"has_attachments":true}],"#,
            r#""selection":{"thread":{"account":"work","thread_id":"th-7"}},"#,
            r#""visible_limit":100,"view_mode":"threaded"}"#
        )
    );

    let back: MailboxListSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn a_snapshot_at_rest_crosses_without_the_list_fields() {
    // A list not showing messages: an empty flat window with nothing selected,
    // the state a folder pane built alone is in. Every list field is absent from
    // the wire, so an emitter that projects no rows crosses byte-identically to
    // the shape the snapshot had before the list fields existed.
    let json = serde_json::to_string(&MailboxListSnapshot {
        account_folders: vec![AccountFolderRow {
            account_id: account("home"),
            display_name: "Home".to_owned(),
            expanded: false,
            can_edit_folders: false,
            folders: vec![],
        }],
        rows: Vec::new(),
        selection: None,
        visible_limit: 0,
        view_mode: ViewMode::Flat,
        search: None,
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"account_folders":[{"account_id":"home","display_name":"Home","expanded":false,"folders":[]}]}"#
    );
}

#[test]
fn a_wire_with_no_list_fields_parses_to_the_at_rest_snapshot() {
    // The additive direction: an old emitter's JSON, carrying none of the list
    // fields, parses to the at-rest snapshot rather than failing.
    let back: MailboxListSnapshot = serde_json::from_str(
        r#"{"account_folders":[{"account_id":"home","display_name":"Home","expanded":false,"folders":[]}]}"#,
    )
    .unwrap();
    assert_eq!(back.account_folders.len(), 1);
    assert!(back.rows.is_empty());
    assert_eq!(back.selection, None);
    assert_eq!(back.visible_limit, 0);
    assert_eq!(back.view_mode, ViewMode::Flat);
}

#[test]
fn the_snapshot_carries_every_account_s_tree_unchanged() {
    // The pane is fed from the snapshot alone, so whatever trees went in are the trees on
    // screen: order, nesting, counts and all, for every account at once.
    let account_folders = vec![
        AccountFolderRow {
            account_id: account("work"),
            display_name: "me@work.example".to_owned(),
            expanded: true,
            can_edit_folders: false,
            folders: vec![
                inbox_row(2),
                FolderRow {
                    key: folder("Clients"),
                    name: "Clients".to_owned(),
                    role: None,
                    unread: 0,
                    parent: None,
                    depth: 0,
                    has_children: true,
                    expanded: true,
                },
                FolderRow {
                    key: folder("Clients/Acme"),
                    name: "Acme".to_owned(),
                    role: None,
                    unread: 1,
                    parent: Some(folder("Clients")),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
            ],
        },
        AccountFolderRow {
            account_id: account("home"),
            display_name: "me@home.example".to_owned(),
            expanded: false,
            can_edit_folders: false,
            folders: vec![inbox_row(1)],
        },
    ];

    let snapshot = MailboxListSnapshot {
        account_folders: account_folders.clone(),
        ..Default::default()
    };

    assert_eq!(snapshot.account_folders, account_folders);
    assert_eq!(snapshot.account_folders[0].folders.len(), 3);
    assert_eq!(
        snapshot.account_folders[0].folders[2].parent,
        Some(folder("Clients"))
    );
    assert_eq!(snapshot.account_folders[1].folders[0].unread, 1);
}
