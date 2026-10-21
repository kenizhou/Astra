// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the method registry: every method the object model names, what it
//! costs, and how stable it is. The registry is the router's manifest-check
//! partner, so an entry drifting from the plan fails here.

use super::*;

#[test]
fn every_registry_method_resolves_by_its_dotted_name() {
    for def in METHODS {
        let found = lookup(def.name).expect("registry method resolves");
        assert_eq!(found.name, def.name);
        assert_eq!(found.node, def.node);
        assert_eq!(found.permission, def.permission);
        assert_eq!(found.stability, def.stability);
    }
}

#[test]
fn the_stable_methods_map_to_intents_that_exist_today() {
    // Stable means the backing intent already exists; SP3b wires the rest.
    // If an intent is renamed or removed, this list is where the drift shows.
    for name in [
        "message.markRead",
        "message.setFlagged",
        "message.archive",
        "message.moveToTrash",
        "message.reply",
        "message.replyAll",
        "message.forward",
        "folder.createSubfolder",
        "explorer.selectFolder",
        "explorer.openMessage",
        "session.getItemFromID",
        "session.getFolderFromID",
    ] {
        let def = lookup(name).unwrap_or_else(|| panic!("{name} must be registered"));
        assert_eq!(
            def.stability,
            Stability::Stable,
            "{name} maps to an existing intent and must be stable"
        );
    }
}

#[test]
fn the_send_and_transform_methods_cost_the_split_permissions() {
    assert_eq!(
        lookup("message.send").expect("send").permission,
        Permission::Send
    );
    assert_eq!(
        lookup("composer.transform").expect("transform").permission,
        Permission::ComposeEdit
    );
    assert_eq!(
        lookup("composer.setOption").expect("setOption").permission,
        Permission::ComposeEdit
    );
    assert_eq!(
        lookup("before_send").expect("before_send").permission,
        Permission::Send
    );
}

#[test]
fn the_ai_methods_sit_behind_ai_infer() {
    for name in [
        "agent.summarize",
        "agent.classify",
        "agent.replySuggest",
        "agent.composeAssist",
        "agent.agentTask",
    ] {
        let def = lookup(name).unwrap_or_else(|| panic!("{name} must be registered"));
        assert_eq!(def.permission, Permission::AiInfer, "{name} costs ai.infer");
        assert_eq!(def.stability, Stability::Proposed);
    }
}

#[test]
fn an_unknown_name_resolves_to_nothing() {
    assert!(lookup("message.exfiltrate").is_none());
    assert!(lookup("").is_none());
}

#[test]
fn the_custom_plugin_namespace_is_recognised() {
    assert!(is_custom_method("plugin.dev.astra.echo.ping"));
    assert!(!is_custom_method("message.markRead"));
    assert!(!is_custom_method("plugin"));
    assert!(!is_custom_method("plugin."));
}

#[test]
fn every_node_appears_in_the_registry_or_is_named_absent() {
    // The registry covers the nodes this slice routes; nodes with no methods
    // yet (Calendar, People, Outbox, Settings, Inspector, Attachment,
    // Recipient, Plugins) arrive with their intents and must not be looked up
    // accidentally.
    for def in METHODS {
        assert!(
            matches!(
                def.node,
                NodeKind::AstraMail
                    | NodeKind::Session
                    | NodeKind::Folder
                    | NodeKind::Message
                    | NodeKind::Explorer
                    | NodeKind::Composer
                    | NodeKind::Agent
            ),
            "method {} sits on an unexpected node {:?}",
            def.name,
            def.node
        );
    }
}
