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
        (Permission::ReadMailbox, "read_mailbox"),
        (Permission::TransformOutgoing, "transform_outgoing"),
        (Permission::DecodeIncoming, "decode_incoming"),
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
