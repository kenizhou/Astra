// SPDX-License-Identifier: GPL-3.0-only
//! Wire tests for the drafts family's by-key arm, split from
//! `protocol_tests.rs` so that file stays under the 500-line cap. The
//! composition-keyed verbs cross there; the row-keyed discard crosses here.

use super::{DraftsIntent, Intent};
use crate::reference::MessageRef;

/// The row's reference to a stored draft, the way these tests name one.
fn draft_ref(key: &str) -> MessageRef {
    MessageRef::from_parts("work", key.to_owned()).unwrap()
}

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

#[test]
fn the_stored_discard_crosses_with_the_message_reference() {
    round_trips(
        &Intent::Drafts(DraftsIntent::DiscardStored {
            message: draft_ref("d-1"),
        }),
        r#"{"drafts":{"discard_stored":{"message":{"account":"work","key":"d-1"}}}}"#,
    );
}

#[test]
fn a_stored_discard_with_a_malformed_reference_is_refused_on_the_wire() {
    let blank = r#"{"drafts":{"discard_stored":{"message":{"account":"work","key":""}}}}"#;
    assert!(serde_json::from_str::<Intent>(blank).is_err());
}
