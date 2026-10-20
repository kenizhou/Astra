// SPDX-License-Identifier: GPL-3.0-only
//! The folder family's wire shapes: every folder intent and the
//! move-to-folder batch, crossing the IPC and reading back identical. Split
//! from the protocol tests so each file stays under the line cap.

use engine_api::AccountId;

use super::{FolderIntent, Intent};
use crate::reference::{FolderRef, MessageRef};

/// A well-formed account id, the way every test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

#[test]
fn folders_carries_the_create_intent() {
    // A top-level create omits `parent` entirely; a nested one carries it.
    round_trips(
        &Intent::Folders(FolderIntent::Create {
            account: account("work"),
            parent: None,
            name: "Archive".to_owned(),
        }),
        r#"{"folders":{"create":{"account":"work","name":"Archive"}}}"#,
    );
    round_trips(
        &Intent::Folders(FolderIntent::Create {
            account: account("work"),
            parent: Some("INBOX".to_owned()),
            name: "Archive".to_owned(),
        }),
        r#"{"folders":{"create":{"account":"work","parent":"INBOX","name":"Archive"}}}"#,
    );
}

#[test]
fn folders_carries_the_rename_intent() {
    round_trips(
        &Intent::Folders(FolderIntent::Rename {
            account: account("work"),
            folder: "clients".to_owned(),
            name: "Customers".to_owned(),
        }),
        r#"{"folders":{"rename":{"account":"work","folder":"clients","name":"Customers"}}}"#,
    );
}

#[test]
fn folders_carries_the_move_intent() {
    // A move to the top level omits `parent` entirely; a nested one carries it.
    round_trips(
        &Intent::Folders(FolderIntent::Move {
            account: account("work"),
            folder: "archive/clients".to_owned(),
            parent: None,
        }),
        r#"{"folders":{"move":{"account":"work","folder":"archive/clients"}}}"#,
    );
    round_trips(
        &Intent::Folders(FolderIntent::Move {
            account: account("work"),
            folder: "clients".to_owned(),
            parent: Some("archive".to_owned()),
        }),
        r#"{"folders":{"move":{"account":"work","folder":"clients","parent":"archive"}}}"#,
    );
}

#[test]
fn folders_carries_the_trash_and_delete_intents() {
    round_trips(
        &Intent::Folders(FolderIntent::Trash {
            account: account("work"),
            folder: "clients".to_owned(),
        }),
        r#"{"folders":{"trash":{"account":"work","folder":"clients"}}}"#,
    );
    round_trips(
        &Intent::Folders(FolderIntent::Delete {
            account: account("work"),
            folder: "trash/clients".to_owned(),
        }),
        r#"{"folders":{"delete":{"account":"work","folder":"trash/clients"}}}"#,
    );
}

#[test]
fn move_messages_carries_the_rows_and_the_destination() {
    round_trips(
        &Intent::MoveMessages {
            rows: vec![
                MessageRef::from_parts("work", "m1".to_owned()).unwrap(),
                MessageRef::from_parts("work", "m2".to_owned()).unwrap(),
            ],
            folder: FolderRef::from_parts("work", "clients".to_owned()).unwrap(),
        },
        r#"{"move_messages":{"rows":[{"account":"work","key":"m1"},{"account":"work","key":"m2"}],"folder":{"account":"work","key":"clients"}}}"#,
    );
}
