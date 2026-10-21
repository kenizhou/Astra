// SPDX-License-Identifier: GPL-3.0-only
//! Wire pins for the snapshot structs and the blob reference: the immutable
//! read shapes the host answers object calls with.

use super::*;

use serde_json::json;

#[test]
fn the_message_snapshot_pins_its_wire_shape() {
    let snapshot = MessageSnapshot {
        subject: "Quarterly report".to_string(),
        from: "ada@example.test".to_string(),
        to: vec!["tim@example.test".to_string()],
        cc: vec![],
        sent_at: Some("2026-10-21T09:00:00Z".to_string()),
        unread: true,
        flagged: false,
        has_attachments: false,
        body_text: None,
    };
    assert_eq!(
        serde_json::to_value(&snapshot).expect("serialises"),
        json!({
            "subject": "Quarterly report",
            "from": "ada@example.test",
            "to": ["tim@example.test"],
            "cc": [],
            "sent_at": "2026-10-21T09:00:00Z",
            "unread": true,
            "flagged": false,
            "has_attachments": false,
        })
    );
}

#[test]
fn the_folder_and_account_snapshots_pin_their_shapes() {
    let folder = FolderSnapshot {
        name: "Inbox".to_string(),
        role: "inbox".to_string(),
        unread_count: 3,
    };
    assert_eq!(
        serde_json::to_value(&folder).expect("serialises"),
        json!({ "name": "Inbox", "role": "inbox", "unread_count": 3 })
    );
    let account = AccountSnapshot {
        display_name: "Work".to_string(),
        email: "ada@example.test".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&account).expect("serialises"),
        json!({ "display_name": "Work", "email": "ada@example.test" })
    );
}

#[test]
fn the_blob_reference_pins_its_wire_shape() {
    let blob = BlobRef {
        blob_id: "b7f2".to_string(),
        size: 5_242_880,
        sha256: "9f86d081".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&blob).expect("serialises"),
        json!({ "blob_id": "b7f2", "size": 5_242_880_u64, "sha256": "9f86d081" })
    );
}
