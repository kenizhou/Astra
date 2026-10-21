// SPDX-License-Identifier: GPL-3.0-only
//! Wire pins for the envelope family: one compact JSON object per line,
//! `type`-tagged, snake_case. These pins are the worker protocol's
//! breaking-change alarm, and they prove the family that replaces the interim
//! protocol.rs is the one the plan names.

use super::*;

use crate::{AOM_VERSION, PLUGIN_API_LEVEL};

use serde_json::json;

fn manifest() -> PluginManifest {
    PluginManifest::parse(
        r#"
id = "dev.astra.echo"
name = "Echo"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.dev.astra.echo.ping"]
permissions = []
"#,
    )
    .expect("echo manifest parses")
}

#[test]
fn plugin_init_carries_manifest_grants_versions_and_data_dir() {
    let message = HostMessage::PluginInit {
        manifest: manifest(),
        grants: vec![Permission::ReadItem],
        api_level: PLUGIN_API_LEVEL,
        aom_version: AOM_VERSION,
        data_dir: "C:/plugins/echo".to_string(),
    };
    let value = serde_json::to_value(&message).expect("serialises");
    assert_eq!(value["type"], json!("plugin_init"));
    assert_eq!(value["grants"], json!(["read_item"]));
    assert_eq!(value["api_level"], json!(1));
    assert_eq!(value["aom_version"], json!(1));
    assert_eq!(value["data_dir"], json!("C:/plugins/echo"));
    assert_eq!(value["manifest"]["id"], json!("dev.astra.echo"));
    let round: HostMessage = serde_json::from_value(value).expect("deserialises");
    assert_eq!(round, message);
}

#[test]
fn object_call_carries_id_handle_method_and_args() {
    let message = HostMessage::ObjectCall {
        id: "c1".to_string(),
        handle: Some(ObjectRef::new("msg.dev.astra.echo.3.7".to_string())),
        method: "message.markRead".to_string(),
        args: json!([true]),
    };
    assert_eq!(
        serde_json::to_value(&message).expect("serialises"),
        json!({
            "type": "object_call",
            "id": "c1",
            "handle": "msg.dev.astra.echo.3.7",
            "method": "message.markRead",
            "args": [true],
        })
    );
}

#[test]
fn object_call_omits_a_missing_handle() {
    let message = HostMessage::ObjectCall {
        id: "c2".to_string(),
        handle: None,
        method: "session.getItemFromID".to_string(),
        args: json!([]),
    };
    assert_eq!(
        serde_json::to_value(&message).expect("serialises"),
        json!({
            "type": "object_call",
            "id": "c2",
            "method": "session.getItemFromID",
            "args": [],
        })
    );
}

#[test]
fn subscribe_and_shutdown_pin_their_shapes() {
    let subscribe = HostMessage::ObjectSubscribe {
        id: "s1".to_string(),
        stream: "explorer.onSelectionChange".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&subscribe).expect("serialises"),
        json!({ "type": "object_subscribe", "id": "s1", "stream": "explorer.onSelectionChange" })
    );
    assert_eq!(
        serde_json::to_value(&HostMessage::PluginShutdown).expect("serialises"),
        json!({ "type": "plugin_shutdown" })
    );
}

#[test]
fn plugin_ready_names_its_versions_and_methods() {
    let message = WorkerMessage::PluginReady {
        api_level: PLUGIN_API_LEVEL,
        aom_version: AOM_VERSION,
        methods: vec!["plugin.dev.astra.echo.ping".to_string()],
    };
    assert_eq!(
        serde_json::to_value(&message).expect("serialises"),
        json!({
            "type": "plugin_ready",
            "api_level": 1,
            "aom_version": 1,
            "methods": ["plugin.dev.astra.echo.ping"],
        })
    );
}

#[test]
fn result_error_event_and_permission_request_pin_their_shapes() {
    let result = WorkerMessage::ObjectResult {
        id: "c1".to_string(),
        value: json!({ "ok": true }),
    };
    assert_eq!(
        serde_json::to_value(&result).expect("serialises"),
        json!({ "type": "object_result", "id": "c1", "value": { "ok": true } })
    );
    let error = WorkerMessage::ObjectError {
        id: "c1".to_string(),
        message: "no such handle".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&error).expect("serialises"),
        json!({ "type": "object_error", "id": "c1", "message": "no such handle" })
    );
    let event = WorkerMessage::ObjectEvent {
        stream: "explorer.onSelectionChange".to_string(),
        event: json!({ "selected": 2 }),
    };
    assert_eq!(
        serde_json::to_value(&event).expect("serialises"),
        json!({
            "type": "object_event",
            "stream": "explorer.onSelectionChange",
            "event": { "selected": 2 },
        })
    );
    let request = WorkerMessage::PermissionRequest {
        id: "p1".to_string(),
        capability: Permission::Send,
        reason: "sign before send".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&request).expect("serialises"),
        json!({
            "type": "permission_request",
            "id": "p1",
            "capability": "send",
            "reason": "sign before send",
        })
    );
    let plugin_event = WorkerMessage::PluginEvent {
        event: json!({ "kind": "pong" }),
    };
    assert_eq!(
        serde_json::to_value(&plugin_event).expect("serialises"),
        json!({ "type": "plugin_event", "event": { "kind": "pong" } })
    );
}
