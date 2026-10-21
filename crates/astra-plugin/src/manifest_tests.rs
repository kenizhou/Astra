// SPDX-License-Identifier: GPL-3.0-only
//! Tests for [`PluginManifest`]: the parse contract a plugin author meets first.
//!
//! A manifest that lies about its API level or AOM version, names a method the
//! registry does not define, asks for a method without the permission that
//! method costs, or smuggles in an unknown field is rejected at parse. A plugin
//! whose manifest never reaches a host can never run, so the parse IS the
//! security boundary the rest of the host builds on.

use super::*;

const VALID: &str = r#"
id = "dev.astra.crypto"
name = "Astra Crypto"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["message.markRead", "composer.transform", "plugin.dev.astra.crypto.ping"]
permissions = ["read_mailbox", "compose_edit"]
"#;

#[test]
fn a_valid_manifest_parses_with_every_field() {
    let manifest = PluginManifest::parse(VALID).expect("valid manifest");
    assert_eq!(manifest.id.as_str(), "dev.astra.crypto");
    assert_eq!(manifest.name, "Astra Crypto");
    assert_eq!(manifest.version, "0.1.0");
    assert_eq!(manifest.api_level, PLUGIN_API_LEVEL);
    assert_eq!(manifest.aom_version, AOM_VERSION);
    assert_eq!(
        manifest.methods,
        vec![
            "message.markRead".to_string(),
            "composer.transform".to_string(),
            "plugin.dev.astra.crypto.ping".to_string(),
        ]
    );
    assert_eq!(
        manifest.permissions,
        vec![Permission::ReadMailbox, Permission::ComposeEdit,]
    );
}

#[test]
fn an_api_level_above_or_below_the_host_level_is_rejected() {
    for level in [0, 2] {
        let text = VALID.replace("api_level = 1", &format!("api_level = {level}"));
        let error = PluginManifest::parse(&text).expect_err("wrong API level");
        assert!(matches!(
            error,
            ManifestError::ApiLevelMismatch {
                found,
                expected: PLUGIN_API_LEVEL,
            } if found == level
        ));
    }
}

#[test]
fn an_aom_version_other_than_the_host_version_is_rejected() {
    let text = VALID.replace("aom_version = 1", "aom_version = 2");
    let error = PluginManifest::parse(&text).expect_err("wrong AOM version");
    assert!(matches!(
        error,
        ManifestError::AomVersionMismatch {
            found: 2,
            expected: AOM_VERSION,
        }
    ));
}

#[test]
fn a_manifest_with_no_methods_is_rejected() {
    let text = VALID.replace(
        "methods = [\"message.markRead\", \"composer.transform\", \"plugin.dev.astra.crypto.ping\"]",
        "methods = []",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::NoMethods)
    ));
}

#[test]
fn an_unknown_method_is_rejected() {
    let text = VALID.replace("message.markRead", "message.exfiltrate");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::UnknownMethod(method)) if method == "message.exfiltrate"
    ));
}

#[test]
fn a_custom_namespaced_method_is_accepted() {
    // `plugin.<id>.<name>` is the open route for methods no registry entry
    // will ever name; the parse accepts it without a registry lookup.
    let text = VALID.replace("message.markRead", "plugin.anything.at.all");
    assert!(PluginManifest::parse(&text).is_ok());
}

#[test]
fn a_method_declared_twice_is_rejected() {
    let text = VALID.replace("\"plugin.dev.astra.crypto.ping\"", "\"message.markRead\"");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::DuplicateMethod(method)) if method == "message.markRead"
    ));
}

#[test]
fn a_permission_declared_twice_is_rejected() {
    let text = VALID.replace(
        "permissions = [\"read_mailbox\", \"compose_edit\"]",
        "permissions = [\"read_mailbox\", \"compose_edit\", \"read_mailbox\"]",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::DuplicatePermission(Permission::ReadMailbox))
    ));
}

#[test]
fn a_method_without_its_required_permission_is_rejected() {
    let text = VALID.replace(
        "permissions = [\"read_mailbox\", \"compose_edit\"]",
        "permissions = [\"read_mailbox\"]",
    );
    let error = PluginManifest::parse(&text).expect_err("compose_edit undeclared");
    assert!(matches!(
        error,
        ManifestError::MissingPermission {
            ref method,
            permission: Permission::ComposeEdit,
        } if method == "composer.transform"
    ));
}

#[test]
fn a_blank_or_padded_id_is_rejected() {
    for id in ["", "  ", " dev.astra.crypto", "dev.astra.crypto "] {
        let text = VALID.replace("id = \"dev.astra.crypto\"", &format!("id = \"{id}\""));
        assert!(
            matches!(PluginManifest::parse(&text), Err(ManifestError::BlankId)),
            "id {id:?} must be rejected"
        );
    }
}

#[test]
fn an_unknown_field_is_rejected() {
    let text = format!("{VALID}extra = true\n");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::InvalidToml(_))
    ));
}

#[test]
fn an_entrypoint_is_optional_and_round_trips() {
    assert_eq!(
        PluginManifest::parse(VALID).expect("valid").entrypoint,
        None,
        "absent entrypoint parses as None and is omitted on the wire"
    );
    let text = format!("{VALID}entrypoint = \"crypto-worker.exe\"\n");
    let manifest = PluginManifest::parse(&text).expect("with entrypoint");
    assert_eq!(manifest.entrypoint.as_deref(), Some("crypto-worker.exe"));
    let value = serde_json::to_value(&manifest).expect("serialises");
    assert_eq!(value["entrypoint"], serde_json::json!("crypto-worker.exe"));
}

#[test]
fn the_retired_hooks_field_is_rejected() {
    // The hook era is over: a manifest written against hook_api v1 must fail
    // loudly (an unknown field or a missing api_level), never parse silently.
    let text = r#"
id = "dev.astra.old"
name = "Old"
version = "0.1.0"
hook_api = 1
aom_version = 1
hooks = ["compose.transform"]
methods = ["message.markRead"]
permissions = ["read_mailbox"]
"#;
    assert!(PluginManifest::parse(text).is_err());
}

#[test]
fn before_send_requires_its_mode_and_the_mode_requires_the_method() {
    let gated = VALID.replace(
        "methods = [\"message.markRead\", \"composer.transform\", \"plugin.dev.astra.crypto.ping\"]",
        "methods = [\"before_send\"]",
    );
    let gated = gated.replace(
        "permissions = [\"read_mailbox\", \"compose_edit\"]",
        "permissions = [\"send\", \"admin\"]",
    );
    // The handler with no mode declared is rejected.
    assert!(matches!(
        PluginManifest::parse(&gated),
        Err(ManifestError::MissingSendMode)
    ));
    // The mode with no handler declared is rejected.
    let stray = VALID.replace(
        "permissions = [\"read_mailbox\", \"compose_edit\"]",
        "permissions = [\"read_mailbox\", \"compose_edit\"]\nbefore_send_mode = \"block\"",
    );
    assert!(matches!(
        PluginManifest::parse(&stray),
        Err(ManifestError::UnexpectedSendMode)
    ));
}

#[test]
fn the_send_modes_parse_and_round_trip() {
    for (name, mode) in [
        ("block", SendMode::Block),
        ("soft_block", SendMode::SoftBlock),
        ("prompt_user", SendMode::PromptUser),
    ] {
        let text = VALID.replace(
            "methods = [\"message.markRead\", \"composer.transform\", \"plugin.dev.astra.crypto.ping\"]",
            "methods = [\"before_send\"]",
        );
        let text = text.replace(
            "permissions = [\"read_mailbox\", \"compose_edit\"]",
            "permissions = [\"send\", \"admin\"]",
        );
        let text = format!("{text}before_send_mode = \"{name}\"\n");
        let manifest = PluginManifest::parse(&text).expect("mode parses");
        assert_eq!(manifest.before_send_mode, Some(mode));
        let value = serde_json::to_value(mode).expect("serialises");
        assert_eq!(value, serde_json::json!(name));
    }
}
