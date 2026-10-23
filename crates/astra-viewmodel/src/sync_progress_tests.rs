// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sync-progress view-model: the serde wire shape a host pulls, and the
//! default an app publishes before any pass has run.

use engine_api::AccountId;

use crate::{AccountSyncProgress, AccountSyncState, DeliveryState, SyncProgressSnapshot};

#[test]
fn the_wire_shape_is_snake_case_and_skips_an_absent_detail() {
    let snapshot = SyncProgressSnapshot {
        accounts: vec![AccountSyncProgress {
            account: AccountId::try_from("user@example.test").expect("an account id"),
            state: AccountSyncState::Syncing,
            detail: Some("folders 3 of 10".to_owned()),
        }],
        delivery: DeliveryState::default(),
    };

    let wire = serde_json::to_value(&snapshot).expect("the snapshot serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "accounts": [
                {
                    "account": "user@example.test",
                    "state": "syncing",
                    "detail": "folders 3 of 10"
                }
            ],
            "delivery": { "offline_banner": false }
        }),
    );

    let back: SyncProgressSnapshot = serde_json::from_value(wire).expect("the snapshot reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn the_delivery_state_carries_the_banner_and_the_outage_accounts() {
    // The banner and the badge rows ride the snapshot the progress bar
    // already crosses, so the words they spell are pinned here the way the
    // progress words above are.
    let delivery = DeliveryState {
        offline_banner: true,
        outages: vec![
            AccountId::try_from("user@example.test").expect("an account id"),
            AccountId::try_from("work@example.test").expect("an account id"),
        ],
    };
    let wire = serde_json::to_value(&delivery).expect("the state serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "offline_banner": true,
            "outages": ["user@example.test", "work@example.test"]
        }),
    );
    let back: DeliveryState = serde_json::from_value(wire).expect("the state reads back");
    assert_eq!(back, delivery);

    // A snapshot from a build before the field reads back healthy: no
    // delivery key, no outage.
    let legacy = serde_json::json!({
        "accounts": [
            {
                "account": "user@example.test",
                "state": "idle"
            }
        ]
    });
    let back: SyncProgressSnapshot =
        serde_json::from_value(legacy).expect("a snapshot without the delivery key reads");
    assert_eq!(back.delivery, DeliveryState::default());
}

#[test]
fn an_entry_without_a_detail_omits_the_field_on_the_wire() {
    let snapshot = SyncProgressSnapshot {
        accounts: vec![AccountSyncProgress {
            account: AccountId::try_from("user@example.test").expect("an account id"),
            state: AccountSyncState::Idle,
            detail: None,
        }],
        delivery: DeliveryState::default(),
    };

    let wire = serde_json::to_value(&snapshot).expect("the snapshot serialises");
    assert!(
        wire["accounts"][0].get("detail").is_none(),
        "an absent detail crosses no field: {wire}"
    );
}

#[test]
fn a_fresh_snapshot_is_empty_and_round_trips() {
    let snapshot = SyncProgressSnapshot::default();

    assert!(snapshot.accounts.is_empty());
    let back: SyncProgressSnapshot =
        serde_json::from_value(serde_json::to_value(&snapshot).expect("serialises"))
            .expect("reads back");
    assert_eq!(back, snapshot);
}
