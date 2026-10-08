// SPDX-License-Identifier: GPL-3.0-only
//! Tests for [`PluginManifest`]: the parse contract a plugin author meets first.
//!
//! A manifest that lies about its hook API level, names a hook it lacks the
//! permission for, or smuggles in an unknown field is rejected at parse. A plugin
//! whose manifest never reaches a host can never run, so the parse IS the security
//! boundary the rest of the host builds on.

use super::*;

const VALID: &str = r#"
id = "dev.astra.crypto"
name = "Astra Crypto"
version = "0.1.0"
hook_api = 1
hooks = ["compose.transform", "message.decode"]
permissions = ["transform_outgoing", "decode_incoming", "read_mailbox"]
"#;

#[test]
fn a_valid_manifest_parses_with_every_field() {
    let manifest = PluginManifest::parse(VALID).expect("valid manifest");
    assert_eq!(manifest.id.as_str(), "dev.astra.crypto");
    assert_eq!(manifest.name, "Astra Crypto");
    assert_eq!(manifest.version, "0.1.0");
    assert_eq!(manifest.hook_api, HOOK_API_V1);
    assert_eq!(
        manifest.hooks,
        vec![HookApiV1::ComposeTransform, HookApiV1::MessageDecode]
    );
    assert_eq!(
        manifest.permissions,
        vec![
            Permission::TransformOutgoing,
            Permission::DecodeIncoming,
            Permission::ReadMailbox,
        ]
    );
}

#[test]
fn an_api_level_above_the_host_level_is_rejected() {
    let text = VALID.replace("hook_api = 1", "hook_api = 2");
    let error = PluginManifest::parse(&text).expect_err("newer API level");
    assert!(matches!(
        error,
        ManifestError::ApiLevelMismatch {
            found: 2,
            expected: HOOK_API_V1,
        }
    ));
}

#[test]
fn an_api_level_below_the_host_level_is_rejected() {
    let text = VALID.replace("hook_api = 1", "hook_api = 0");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::ApiLevelMismatch { found: 0, .. })
    ));
}

#[test]
fn a_missing_hook_api_field_is_rejected() {
    let text = VALID.replace("hook_api = 1\n", "");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::InvalidToml(_))
    ));
}

#[test]
fn a_hook_without_its_permission_is_rejected() {
    // compose.transform without transform_outgoing: the manifest asks the host to
    // hand it outgoing mail it holds no permission to transform.
    let text = VALID.replace(
        "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]",
        "permissions = [\"decode_incoming\", \"read_mailbox\"]",
    );
    let error = PluginManifest::parse(&text).expect_err("hook without permission");
    assert!(matches!(
        error,
        ManifestError::MissingPermission {
            hook: HookApiV1::ComposeTransform,
            permission: Permission::TransformOutgoing,
        }
    ));
}

#[test]
fn a_decode_hook_without_decode_incoming_is_rejected() {
    let text = VALID.replace(
        "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]",
        "permissions = [\"transform_outgoing\"]",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::MissingPermission {
            hook: HookApiV1::MessageDecode,
            permission: Permission::DecodeIncoming,
        })
    ));
}

#[test]
fn an_unknown_field_is_rejected() {
    let text = format!("{VALID}\nextra_field = \"surprise\"\n");
    let error = PluginManifest::parse(&text).expect_err("unknown field");
    assert!(
        error.to_string().contains("unknown field"),
        "error names the unknown field: {error}"
    );
}

#[test]
fn an_unknown_permission_name_is_rejected() {
    let text = VALID.replace(
        "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]",
        "permissions = [\"transform_outgoing\", \"root\"]",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::InvalidToml(_))
    ));
}

#[test]
fn an_unknown_hook_name_is_rejected() {
    let text = VALID.replace(
        "hooks = [\"compose.transform\", \"message.decode\"]",
        "hooks = [\"engine.write_raw\"]",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::InvalidToml(_))
    ));
}

#[test]
fn a_missing_required_field_is_rejected() {
    let text = VALID.replace("name = \"Astra Crypto\"\n", "");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::InvalidToml(_))
    ));
}

#[test]
fn a_manifest_without_a_permissions_key_is_rejected() {
    // A plugin that declares no permissions at all asks for nothing, which the
    // contract refuses: the key must be present, even before its contents are
    // checked against the declared hooks.
    let text = VALID.replace(
        "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]\n",
        "",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::InvalidToml(_))
    ));
}

#[test]
fn an_empty_id_is_rejected() {
    let text = VALID.replace("dev.astra.crypto", "");
    let error = PluginManifest::parse(&text).expect_err("empty id");
    assert!(matches!(error, ManifestError::BlankId));
}

#[test]
fn a_whitespace_only_id_is_rejected() {
    let text = VALID.replace("dev.astra.crypto", "   ");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::BlankId)
    ));
}

#[test]
fn an_id_with_surrounding_whitespace_is_rejected_not_trimmed() {
    // Pinned behaviour: the guard rejects rather than trims. The host keys
    // registry entries and permission grants by the exact id string, so a
    // silently trimmed id would let two manifests share one identity.
    let text = VALID.replace("dev.astra.crypto", "  dev.astra.crypto  ");
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::BlankId)
    ));
}

#[test]
fn a_duplicate_hook_is_rejected() {
    let text = VALID.replace(
        "hooks = [\"compose.transform\", \"message.decode\"]",
        "hooks = [\"compose.transform\", \"compose.transform\"]",
    );
    let error = PluginManifest::parse(&text).expect_err("duplicate hook");
    assert!(matches!(
        error,
        ManifestError::DuplicateHook(HookApiV1::ComposeTransform)
    ));
}

#[test]
fn a_duplicate_permission_is_rejected() {
    let text = VALID.replace(
        "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]",
        "permissions = [\"transform_outgoing\", \"transform_outgoing\", \"decode_incoming\"]",
    );
    let error = PluginManifest::parse(&text).expect_err("duplicate permission");
    assert!(matches!(
        error,
        ManifestError::DuplicatePermission(Permission::TransformOutgoing)
    ));
}

#[test]
fn a_manifest_declaring_no_hooks_is_rejected() {
    let text = VALID.replace(
        "hooks = [\"compose.transform\", \"message.decode\"]",
        "hooks = []",
    );
    assert!(matches!(
        PluginManifest::parse(&text),
        Err(ManifestError::NoHooks)
    ));
}

#[test]
fn a_coherent_single_hook_manifest_parses() {
    // The coherence rule only demands that each declared hook's permission be
    // present: read_mailbox (or any extra permission) is never demanded by a
    // hook, and a manifest may serve one hook with exactly that hook's
    // permission. The host grants nothing at parse; permissions are asks.
    let text = VALID
        .replace(
            "hooks = [\"compose.transform\", \"message.decode\"]",
            "hooks = [\"message.decode\"]",
        )
        .replace(
            "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]",
            "permissions = [\"decode_incoming\"]",
        );
    let manifest = PluginManifest::parse(&text).expect("coherent manifest");
    assert_eq!(manifest.hooks, vec![HookApiV1::MessageDecode]);
    assert_eq!(manifest.permissions, vec![Permission::DecodeIncoming]);
}

#[test]
fn debug_renders_the_manifest_without_redaction_gaps() {
    // The manifest is secrets-free by design, so its derived Debug is safe to log
    // today; this pins the invariant so a future secret-bearing field that forgets
    // to redact shows up as a deliberate change in this test.
    let manifest = PluginManifest::parse(VALID).expect("valid manifest");
    let rendered = format!("{manifest:?}");
    assert!(rendered.contains("dev.astra.crypto"));
    assert!(!rendered.contains("Secret"));
}

#[test]
fn the_error_rendering_names_the_missing_permission() {
    let text = VALID.replace(
        "permissions = [\"transform_outgoing\", \"decode_incoming\", \"read_mailbox\"]",
        "permissions = [\"decode_incoming\"]",
    );
    let error = PluginManifest::parse(&text).expect_err("hook without permission");
    let rendered = error.to_string();
    assert!(rendered.contains("compose.transform"), "{rendered}");
    assert!(rendered.contains("transform_outgoing"), "{rendered}");
}
