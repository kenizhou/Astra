// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sending commands: the pull follows the queue's transitions
//! from a submit's arrival to its withdrawal, and every outbox action the
//! panel offers reaches the core over the same JSON the webview sends. The
//! fake app and the state builder live in `commands_fakes`, one compiled copy
//! for every command test module.

use std::sync::Arc;

use astra_viewmodel::{QueuedRow, QueuedState, SendingSnapshot};
use engine_api::AccountId;

use super::sending_of;
use crate::app_state::AppState;
use crate::commands_fakes::{state, FakeApp, FakeStore, ARRIVED_AT};

/// The submit the webview sends with no attachments, shaped like the ipc
/// layer's constructor output: the same submit astra-app's protocol tests
/// pin, one paragraph of one plain run.
fn submit_json() -> serde_json::Value {
    serde_json::json!({
        "submit_rich_mail": {
            "from": "work",
            "to": "you@test.local",
            "cc": "",
            "bcc": "",
            "subject": "On its way",
            "document": {
                "blocks": [{"Paragraph": {"content": [
                    {"Text": {"text": "Sent words", "bold": false, "italic": false,
                              "underline": false, "font_size": null, "color": null,
                              "highlight": null, "link": null}}
                ]}}],
                "attachments": []
            },
            "blobs": []
        }
    })
}

/// The submit with host-resolved attachment bytes, the same shape the wire
/// test pins: the document names a file whose bytes ride beside it.
fn submit_with_blob_json() -> serde_json::Value {
    serde_json::json!({
        "submit_rich_mail": {
            "from": "work",
            "to": "you@test.local",
            "cc": "",
            "bcc": "",
            "subject": "",
            "document": {
                "blocks": [{"Paragraph": {"content": [
                    {"Text": {"text": "With a file", "bold": false, "italic": false,
                              "underline": false, "font_size": null, "color": null,
                              "highlight": null, "link": null}}
                ]}}],
                "attachments": []
            },
            "blobs": [{"handle": "blob-1", "bytes": [1, 2, 3]}]
        }
    })
}

/// One outbox action, as the webview sends it: the verb with the queued
/// reference the row it acted on named.
fn outbox_json(verb: &str, account: &str, op: u64) -> serde_json::Value {
    serde_json::json!({"outbox": {verb: {"queued": {"account": account, "op": op}}}})
}

/// Waits until the sending pull answers `want`, then asserts it settled
/// there: the dispatches run on the runtime, so the test meets them where
/// they land.
fn wait_for_sending(state: &AppState, want: &SendingSnapshot) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while &sending_of(state) != want && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        sending_of(state),
        *want,
        "the pull settles on the app's published snapshot",
    );
}

#[test]
fn the_sending_pull_follows_the_queue_from_submit_to_withdrawal() {
    let app = FakeApp::new();
    let state = state("sending-pull", Arc::clone(&app), FakeStore::accepting());

    // Nothing has been sent: the pull answers the empty queue, never a panic.
    assert_eq!(sending_of(&state), SendingSnapshot::default());

    // Submit through the same JSON the webview sends; the pull settles on the
    // queued row the app published, the first op the fake's queue assigned.
    crate::commands::dispatch_intent(&state, submit_json()).expect("the submit dispatches");
    let queued = SendingSnapshot {
        rows: vec![QueuedRow {
            queued: astra_viewmodel::QueuedKey {
                account: AccountId::try_from("work").expect("an account id"),
                op: 1,
            },
            state: QueuedState::Queued,
            subject: "On its way".to_owned(),
            arrived_at: ARRIVED_AT.to_owned(),
        }],
        held: vec![],
    };
    wait_for_sending(&state, &queued);

    // A show that changes nothing publishes nothing: the pull answers the
    // same snapshot, and no new op was assigned.
    crate::commands::dispatch_intent(&state, serde_json::json!({"outbox": "show"}))
        .expect("the show dispatches");
    wait_for_sending(&state, &queued);

    // Withdraw the send the same way; the row goes with it, and the pull
    // answers the empty queue again.
    crate::commands::dispatch_intent(&state, outbox_json("cancel", "work", 1))
        .expect("the cancel dispatches");
    wait_for_sending(&state, &SendingSnapshot::default());
}

#[test]
fn the_outbox_actions_mark_send_and_settle_or_requeue_the_row() {
    let app = FakeApp::new();
    let state = state("sending-actions", Arc::clone(&app), FakeStore::accepting());

    // The submit with resolved blob bytes queues under the same rules; its
    // subject is the empty one the with-blobs wire literal carries.
    crate::commands::dispatch_intent(&state, submit_with_blob_json())
        .expect("the submit dispatches");
    let key = astra_viewmodel::QueuedKey {
        account: AccountId::try_from("work").expect("an account id"),
        op: 1,
    };
    let stamp = ARRIVED_AT.to_owned();
    wait_for_sending(
        &state,
        &SendingSnapshot {
            rows: vec![QueuedRow {
                queued: key.clone(),
                state: QueuedState::Queued,
                subject: String::new(),
                arrived_at: stamp.clone(),
            }],
            held: vec![],
        },
    );

    // A send now marks the row in flight: the pull carries the transition.
    crate::commands::dispatch_intent(&state, outbox_json("send_now", "work", 1))
        .expect("the send-now dispatches");
    wait_for_sending(
        &state,
        &SendingSnapshot {
            rows: vec![QueuedRow {
                queued: key.clone(),
                state: QueuedState::Sending,
                subject: String::new(),
                arrived_at: stamp.clone(),
            }],
            held: vec![],
        },
    );

    // The answer that it did not arrive puts the send back in the queue.
    crate::commands::dispatch_intent(&state, outbox_json("confirm_not_sent", "work", 1))
        .expect("the not-sent answer dispatches");
    wait_for_sending(
        &state,
        &SendingSnapshot {
            rows: vec![QueuedRow {
                queued: key,
                state: QueuedState::Queued,
                subject: String::new(),
                arrived_at: stamp,
            }],
            held: vec![],
        },
    );

    // The answer that it did arrive settles the send: the row leaves.
    crate::commands::dispatch_intent(&state, outbox_json("confirm_sent", "work", 1))
        .expect("the sent answer dispatches");
    wait_for_sending(&state, &SendingSnapshot::default());
}
