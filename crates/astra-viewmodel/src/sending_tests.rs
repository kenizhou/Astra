// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sending view-model: the exact JSON wire shape the queue
//! snapshot crosses the IPC in.

use engine_api::AccountId;

use super::{HeldSend, HeldState, QueuedKey, QueuedRow, QueuedState, SendingSnapshot};

/// A well-formed account id, the way every sending test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

#[test]
fn a_queued_send_crosses_with_its_key_state_subject_and_arrival() {
    let snapshot = SendingSnapshot {
        rows: vec![QueuedRow {
            queued: QueuedKey {
                account: account("work"),
                op: 3,
            },
            state: QueuedState::Queued,
            subject: "Quarterly review".to_owned(),
            arrived_at: "2026-10-09T08:30:00Z".to_owned(),
        }],
        held: vec![],
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"rows":[{"queued":{"account":"work","op":3},"state":"queued","#,
            r#""subject":"Quarterly review","arrived_at":"2026-10-09T08:30:00Z"}]}"#
        )
    );
    assert_eq!(
        serde_json::from_str::<SendingSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn the_states_cross_as_plain_words() {
    assert_eq!(
        serde_json::to_string(&QueuedState::Queued).unwrap(),
        r#""queued""#
    );
    assert_eq!(
        serde_json::to_string(&QueuedState::Sending).unwrap(),
        r#""sending""#
    );
    assert_eq!(
        serde_json::to_string(&QueuedState::Unconfirmed).unwrap(),
        r#""unconfirmed""#
    );
    assert_eq!(
        serde_json::to_string(&QueuedState::Failed).unwrap(),
        r#""failed""#
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<QueuedState>(r#""Unconfirmed""#).is_err());
}

#[test]
fn an_empty_queue_crosses_as_an_empty_row_list() {
    let snapshot = SendingSnapshot::default();
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(json, r#"{"rows":[]}"#);
    assert_eq!(
        serde_json::from_str::<SendingSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn a_row_round_trips_with_an_empty_subject_and_instant() {
    let row = QueuedRow {
        queued: QueuedKey {
            account: account("home"),
            op: 1,
        },
        state: QueuedState::Failed,
        subject: String::new(),
        arrived_at: String::new(),
    };
    let json = serde_json::to_string(&row).unwrap();
    assert_eq!(
        json,
        r#"{"queued":{"account":"home","op":1},"state":"failed","subject":"","arrived_at":""}"#
    );
    assert_eq!(serde_json::from_str::<QueuedRow>(&json).unwrap(), row);
}

#[test]
fn a_held_send_crosses_with_plugin_reason_and_state() {
    let snapshot = SendingSnapshot {
        rows: vec![],
        held: vec![HeldSend {
            hold: "hold-7".to_owned(),
            account: account("work"),
            subject: "Quarterly review".to_owned(),
            plugin: "dev.astra.crypto".to_owned(),
            reason: "no ready key for tim@example.test".to_owned(),
            state: HeldState::Blocked,
        }],
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"rows":[],"held":[{"hold":"hold-7","account":"work","#,
            r#""subject":"Quarterly review","plugin":"dev.astra.crypto","#,
            r#""reason":"no ready key for tim@example.test","state":"blocked"}]}"#
        )
    );
    assert_eq!(
        serde_json::from_str::<SendingSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn the_held_states_cross_as_plain_words() {
    assert_eq!(
        serde_json::to_string(&HeldState::Checking).unwrap(),
        r#""checking""#
    );
    assert_eq!(
        serde_json::to_string(&HeldState::NeedsAnswer).unwrap(),
        r#""needs_answer""#
    );
    assert_eq!(
        serde_json::to_string(&HeldState::Blocked).unwrap(),
        r#""blocked""#
    );
}
