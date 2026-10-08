// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the mailbox-list snapshot: the exact JSON wire shape it crosses the IPC in,
//! and that it carries every account's folder tree intact.

use engine_api::{AccountId, MailboxId};

use super::*;
use crate::folders::{FolderRole, FolderRow};

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn folder(value: &str) -> MailboxId {
    MailboxId::try_from(value).unwrap()
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
                folders: vec![],
            },
        ],
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
fn the_snapshot_carries_every_account_s_tree_unchanged() {
    // The pane is fed from the snapshot alone, so whatever trees went in are the trees on
    // screen: order, nesting, counts and all, for every account at once.
    let account_folders = vec![
        AccountFolderRow {
            account_id: account("work"),
            display_name: "me@work.example".to_owned(),
            expanded: true,
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
            folders: vec![inbox_row(1)],
        },
    ];

    let snapshot = MailboxListSnapshot {
        account_folders: account_folders.clone(),
    };

    assert_eq!(snapshot.account_folders, account_folders);
    assert_eq!(snapshot.account_folders[0].folders.len(), 3);
    assert_eq!(
        snapshot.account_folders[0].folders[2].parent,
        Some(folder("Clients"))
    );
    assert_eq!(snapshot.account_folders[1].folders[0].unread, 1);
}
