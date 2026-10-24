// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the report family's wire: the exact JSON the two spam-report
//! intents cross the IPC in, and that each reads back identical. Split from
//! the protocol's own tests so that file stays under the line cap; the
//! literals mirror the web client's reports.test.ts pins byte for byte.

use super::Intent;
use crate::reference::{MessageRef, RowRef, ThreadRef};

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

#[test]
fn mark_junk_carries_the_row() {
    round_trips(
        &Intent::MarkJunk {
            row: RowRef::Message(MessageRef::from_parts("work", "imap:v1:u1".to_owned()).unwrap()),
        },
        r#"{"mark_junk":{"row":{"message":{"account":"work","key":"imap:v1:u1"}}}}"#,
    );
    round_trips(
        &Intent::MarkJunk {
            row: RowRef::Thread(ThreadRef::from_parts("work", "th-7".to_owned()).unwrap()),
        },
        r#"{"mark_junk":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}"#,
    );
    // The CamelCase spelling is not the wire: a host must send the renamed word.
    assert!(serde_json::from_str::<Intent>(
        r#"{"MarkJunk":{"row":{"message":{"account":"work","key":"k1"}}}}"#
    )
    .is_err());
}

#[test]
fn mark_not_junk_carries_the_row() {
    round_trips(
        &Intent::MarkNotJunk {
            row: RowRef::Message(MessageRef::from_parts("work", "jmap:e2".to_owned()).unwrap()),
        },
        r#"{"mark_not_junk":{"row":{"message":{"account":"work","key":"jmap:e2"}}}}"#,
    );
    round_trips(
        &Intent::MarkNotJunk {
            row: RowRef::Thread(ThreadRef::from_parts("work", "th-7".to_owned()).unwrap()),
        },
        r#"{"mark_not_junk":{"row":{"thread":{"account":"work","thread_id":"th-7"}}}}"#,
    );
}
