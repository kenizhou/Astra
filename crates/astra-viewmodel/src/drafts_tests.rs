// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the draft-status view-model: the exact JSON wire shape a save's
//! hint crosses the IPC in.

use super::{DraftSaveState, DraftStatusSnapshot};

#[test]
fn a_saved_snapshot_crosses_with_the_state_and_the_instant() {
    let snapshot = DraftStatusSnapshot {
        composition: "compose-1".to_owned(),
        state: DraftSaveState::Saved,
        saved_at: "2026-10-09T08:30:00Z".to_owned(),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        r#"{"composition":"compose-1","state":"saved","saved_at":"2026-10-09T08:30:00Z"}"#
    );
    assert_eq!(
        serde_json::from_str::<DraftStatusSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn a_saving_snapshot_crosses_with_an_empty_instant() {
    let snapshot = DraftStatusSnapshot {
        composition: "compose-2".to_owned(),
        state: DraftSaveState::Saving,
        saved_at: String::new(),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        r#"{"composition":"compose-2","state":"saving","saved_at":""}"#
    );
    assert_eq!(
        serde_json::from_str::<DraftStatusSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn the_states_cross_as_plain_words() {
    assert_eq!(
        serde_json::to_string(&DraftSaveState::Saving).unwrap(),
        r#""saving""#
    );
    assert_eq!(
        serde_json::to_string(&DraftSaveState::Saved).unwrap(),
        r#""saved""#
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<DraftSaveState>(r#""Saved""#).is_err());
}
