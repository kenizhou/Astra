// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading command: the pull returns the snapshot the app
//! published for the open message, and `None` once the pane is closed. Its own
//! file because `commands_tests.rs` sits at the size cap; a child module so the
//! fake app and the state builder above stay shared.

use astra_viewmodel::{AttachmentRow, MessageKey, ReadingSnapshot};
use engine_api::{AccountId, ProviderKey};

use super::*;
use crate::commands::reading_of;

/// The snapshot the fake app publishes for the message an open names: the same
/// shape `astra-viewmodel`'s wire tests pin, carrying one attachment.
pub(super) fn open_snapshot() -> ReadingSnapshot {
    ReadingSnapshot {
        message: MessageKey {
            account: AccountId::try_from("work").expect("an account id"),
            key: ProviderKey::new("imap:v1:u42").expect("a provider key"),
        },
        subject: "Quarterly report".to_owned(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        html: Some("<p>See the report.</p>".to_owned()),
        plain: Some("See the report.".to_owned()),
        attachments: vec![AttachmentRow {
            id: 1,
            file_name: "report.pdf".to_owned(),
            media_type: "application/pdf".to_owned(),
            size: 2048,
        }],
        remote_image_count: 2,
    }
}

/// Waits until the pull answers `want`, then asserts it settled there: the
/// dispatches run on the runtime, so the test meets them where they land.
fn wait_for_reading(state: &AppState, want: Option<&ReadingSnapshot>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while reading_of(state).as_ref() != want && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        reading_of(state),
        want.cloned(),
        "the pull settles on the app's published snapshot",
    );
}

#[test]
fn the_reading_pull_returns_the_published_snapshot_and_none_after_close() {
    let app = FakeApp::new();
    let state = state("reading-pull", Arc::clone(&app), FakeStore::accepting());

    // Open through the same JSON the webview sends, then pull: the pane's
    // snapshot is the app's, whole.
    dispatch_intent(
        &state,
        serde_json::json!({"open_message": {"message": {"account": "work", "key": "imap:v1:u42"}}}),
    )
    .expect("the open dispatches");
    let snapshot = open_snapshot();
    wait_for_reading(&state, Some(&snapshot));

    // Close the pane the same way; the pull answers `None`, never a panic.
    dispatch_intent(&state, serde_json::json!("close_reading")).expect("the close dispatches");
    wait_for_reading(&state, None);
}
