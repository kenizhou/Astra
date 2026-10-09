// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading command: the pull returns the snapshot the app
//! published for the open message, and `None` once the pane is closed. Its own
//! file because `commands_tests.rs` sits at the size cap; the fake app and the
//! state builder live in `commands_fakes`.

use astra_viewmodel::ReadingSnapshot;

use super::*;
use crate::app_state::AppState;
use crate::commands::reading_of;
use crate::commands_fakes::open_snapshot;

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
