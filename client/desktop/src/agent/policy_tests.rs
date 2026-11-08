// SPDX-License-Identifier: GPL-3.0-only
//! The default permission policy's exact rows: a typo in this table
//! silently disables or fail-closes a tool in the live surface (the
//! bridge fails closed on an absent row), so the table is pinned word for
//! word. Carried from the T2 review: nothing else pins it.

use super::protocol::{PermissionLevel, PermissionPolicy};

/// The policy's rows exactly as SPA0 ships them: the read verbs (search
/// joined them with SP2y) run, the draft and act verbs ask.
#[test]
fn the_default_policy_rows_are_exactly_the_shipped_set() {
    let policy = super::default_policy();
    let row = |level: PermissionLevel| PermissionPolicy {
        level,
        allowlist: None,
    };
    let expected: Vec<(&str, PermissionPolicy)> = vec![
        ("list_folders", row(PermissionLevel::Auto)),
        ("list_messages", row(PermissionLevel::Auto)),
        ("read_message", row(PermissionLevel::Auto)),
        ("search_messages", row(PermissionLevel::Auto)),
        ("save_draft", row(PermissionLevel::Confirm)),
        ("discard_draft", row(PermissionLevel::Confirm)),
        ("mark_read", row(PermissionLevel::Confirm)),
        ("set_flagged", row(PermissionLevel::Confirm)),
        ("archive_message", row(PermissionLevel::Confirm)),
        ("move_to_trash", row(PermissionLevel::Confirm)),
    ];
    assert_eq!(
        policy.len(),
        expected.len(),
        "no row may appear or vanish silently: {policy:?}"
    );
    for (tool, want) in expected {
        assert_eq!(
            policy.get(tool),
            Some(&want),
            "the {tool} row must be exactly {want:?}: {policy:?}"
        );
    }
}
