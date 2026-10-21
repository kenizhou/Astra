// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the fail-closed permission table: every permission the surface can
//! name defaults to [`Approval::NeedsApproval`], so the host cannot grant anything
//! a human did not decide on. [`Approval::Granted`] exists only for a future
//! host-side grant path and is deliberately unreachable from this table.

use super::*;

use serde_json::json;

#[test]
fn every_permission_defaults_to_needing_approval() {
    for permission in Permission::ALL {
        assert_eq!(
            default_approval(permission),
            Approval::NeedsApproval,
            "permission {permission:?} must fail closed"
        );
    }
}

#[test]
fn permissions_round_trip_through_their_manifest_names() {
    for (permission, name) in [
        (Permission::ReadItem, "read_item"),
        (Permission::ReadMailbox, "read_mailbox"),
        (Permission::ComposeEdit, "compose_edit"),
        (Permission::Send, "send"),
        (Permission::Admin, "admin"),
        (Permission::AiInfer, "ai.infer"),
    ] {
        assert_eq!(permission.wire_name(), name);
        assert_eq!(
            serde_json::to_value(permission).expect("serialises"),
            json!(name)
        );
        let parsed: Permission = serde_json::from_value(json!(name)).expect("deserialises");
        assert_eq!(parsed, permission);
    }
}

#[test]
fn compose_edit_and_send_are_distinct_powers() {
    // The review's split: holding the composer pen must never imply the power
    // to send, so the two permissions are separate variants with separate
    // grants, and neither's name parses as the other.
    assert_ne!(Permission::ComposeEdit, Permission::Send);
    let parsed: Result<Permission, _> = serde_json::from_value(json!("transform_outgoing"));
    assert!(
        parsed.is_err(),
        "the retired bundled tier must not parse any more"
    );
}
