// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the plugin registry: discovery, the enable/disable lifecycle, the
//! grant table, and shutdown. No real worker spawns here: the enable failure
//! path runs against a manifest whose binary does not exist, which the worker
//! host answers with a fatal spawn error.

use std::path::{Path, PathBuf};

use ring::signature::KeyPair as _;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "astra-desktop-plugin-{}-{}",
        name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Writes one plugin directory under `root` with a manifest naming the echo
/// custom method; the binary is deliberately absent unless `entrypoint` names
/// one (the enable test drives the spawn failure through that absence).
fn write_plugin(root: &Path, id: &str, entrypoint: Option<&str>) -> PathBuf {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).expect("plugin dir");
    let entrypoint = entrypoint
        .map(|name| format!("entrypoint = \"{name}\"\n"))
        .unwrap_or_default();
    std::fs::write(
        dir.join("manifest.toml"),
        format!(
            r#"
id = "{id}"
name = "{id} plugin"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.{id}.ping"]
permissions = ["read_mailbox"]
{entrypoint}"#
        ),
    )
    .expect("manifest written");
    dir
}

fn registry(name: &str, dev_dirs: Vec<PathBuf>) -> PluginRegistry {
    PluginRegistry::new(scratch(name), dev_dirs, true)
}

#[test]
fn discovery_finds_a_valid_plugin_and_resolves_its_program() {
    let registry = registry("discover", vec![]);
    write_plugin(&scratch("discover"), "dev.astra.echo", None);
    let rows = registry.list();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, "dev.astra.echo");
    assert_eq!(row.name, "dev.astra.echo plugin");
    assert_eq!(row.lifecycle, "discovered");
    assert!(row
        .program
        .ends_with(&format!("echo{}", std::env::consts::EXE_SUFFIX)));
}

#[test]
fn a_broken_manifest_is_skipped_by_discovery() {
    let registry = registry("broken", vec![]);
    let dir = scratch("broken").join("dev.astra.broken");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("manifest.toml"), "not a manifest").expect("written");
    write_plugin(&scratch("broken"), "dev.astra.echo", None);
    let rows = registry.list();
    assert_eq!(rows.len(), 1, "only the parseable plugin is discovered");
    assert_eq!(rows[0].id, "dev.astra.echo");
}

#[test]
fn enable_with_a_missing_binary_fails_the_plugin() {
    let registry = registry("enable-fail", vec![]);
    write_plugin(&scratch("enable-fail"), "dev.astra.echo", None);
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            let error = registry
                .enable("dev.astra.echo")
                .await
                .expect_err("the binary is absent");
            assert!(matches!(error, RegistryError::Enable { .. }));
            let row = registry
                .list()
                .into_iter()
                .find(|row| row.id == "dev.astra.echo")
                .expect("row");
            assert_eq!(row.lifecycle, "failed");
        });
}

#[test]
fn enable_of_an_unknown_id_errors() {
    let registry = registry("unknown", vec![]);
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            assert!(matches!(
                registry.enable("dev.astra.ghost").await,
                Err(RegistryError::UnknownPlugin(_))
            ));
        });
}

#[test]
fn disable_requires_an_active_plugin() {
    let registry = registry("disable", vec![]);
    write_plugin(&scratch("disable"), "dev.astra.echo", None);
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            assert!(matches!(
                registry.disable("dev.astra.echo").await,
                Err(RegistryError::NotActive(_))
            ));
        });
}

#[test]
fn approve_adds_a_declared_grant_and_refuses_the_rest() {
    let registry = registry("approve", vec![]);
    write_plugin(&scratch("approve"), "dev.astra.echo", None);
    registry.list();
    registry
        .approve("dev.astra.echo", Permission::ReadMailbox)
        .expect("declared grant");
    assert!(matches!(
        registry.approve("dev.astra.echo", Permission::ReadMailbox),
        Err(RegistryError::AlreadyGranted(_))
    ));
    assert!(matches!(
        registry.approve("dev.astra.echo", Permission::Send),
        Err(RegistryError::UndeclaredPermission { .. })
    ));
    assert!(matches!(
        registry.approve("dev.astra.ghost", Permission::Send),
        Err(RegistryError::UnknownPlugin(_))
    ));
    let row = registry
        .list()
        .into_iter()
        .find(|row| row.id == "dev.astra.echo")
        .expect("row");
    assert_eq!(row.grants, vec!["read_mailbox".to_string()]);
}

#[test]
fn a_dev_dir_participates_in_discovery() {
    let dev = scratch("dev-root");
    write_plugin(&dev, "dev.astra.echo", None);
    // dev_dirs point directly at a plugin directory, not at a scan root.
    let registry = registry("dev", vec![dev.join("dev.astra.echo")]);
    let rows = registry.list();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "dev.astra.echo");
}

#[test]
fn shutdown_all_with_nothing_running_is_quiet() {
    let registry = registry("quiet", vec![]);
    write_plugin(&scratch("quiet"), "dev.astra.echo", None);
    registry.list();
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| registry.shutdown_all());
}

#[test]
fn a_call_reaches_the_worker_and_undeclared_or_inactive_refuse() {
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            let registry = crate::plugin::testkit::registry_with(
                "call",
                "dev.astra.echo",
                &["plugin.dev.astra.echo.ping"],
                None,
                &[],
                crate::plugin::testkit::spawn_echo(),
            )
            .await;
            let answer = registry
                .call(
                    "dev.astra.echo",
                    "plugin.dev.astra.echo.ping",
                    serde_json::json!({"n": 1}),
                )
                .await
                .expect("the echo answers");
            assert_eq!(answer, serde_json::json!({"n": 1}), "the args echoed back");
            let undeclared = registry
                .call("dev.astra.echo", "message.markRead", serde_json::json!([]))
                .await;
            assert!(
                matches!(undeclared, Err(RegistryError::Call { .. })),
                "an undeclared method refuses before the pipe"
            );
            assert!(matches!(
                registry
                    .call(
                        "dev.astra.ghost",
                        "plugin.dev.astra.echo.ping",
                        serde_json::json!([])
                    )
                    .await,
                Err(RegistryError::UnknownPlugin(_))
            ));
        });
}

#[test]
fn grants_and_the_enabled_flag_persist_across_registry_instances() {
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            let root = scratch("persist");
            write_plugin(&root, "dev.astra.echo", None);
            let registry = PluginRegistry::new(root.clone(), vec![], true);
            registry.list();
            registry
                .approve("dev.astra.echo", Permission::ReadMailbox)
                .expect("declared grant");
            registry
                .enable_with("dev.astra.echo", crate::plugin::testkit::spawn_echo())
                .await
                .expect("enabled");

            // A fresh instance over the same root reads the file.
            let reloaded = PluginRegistry::new(root, vec![], true);
            let row = reloaded
                .list()
                .into_iter()
                .find(|row| row.id == "dev.astra.echo")
                .expect("row");
            assert_eq!(row.grants, vec!["read_mailbox".to_string()]);
            assert_eq!(
                reloaded.restore_enabled(),
                vec!["dev.astra.echo".to_string()],
                "the enabled flag survives the restart"
            );
        });
}

#[test]
fn production_refuses_a_plugin_declaring_proposed_methods() {
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            let root = scratch("proposed");
            // before_send is proposed this slice: a release build must not
            // enable a plugin declaring it. The manifest names it properly.
            let dir = root.join("dev.astra.echo");
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(
                dir.join("manifest.toml"),
                r#"
id = "dev.astra.echo"
name = "Echo"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["before_send"]
permissions = ["send", "admin"]
before_send_mode = "block"
"#,
            )
            .expect("manifest written");
            let registry = PluginRegistry::new(root.clone(), vec![], false);
            registry.list();
            let error = registry
                .enable("dev.astra.echo")
                .await
                .expect_err("proposed refused in production");
            assert!(matches!(error, RegistryError::ProposedPlugin(_)));
            // The dev build over the same directory enables it.
            let dev = PluginRegistry::new(root, vec![], true);
            assert!(dev
                .enable_with("dev.astra.echo", crate::plugin::testkit::spawn_echo())
                .await
                .is_ok());
        });
}

#[test]
fn a_malformed_grants_file_reads_as_empty() {
    let root = scratch("malformed-grants");
    write_plugin(&root, "dev.astra.echo", None);
    std::fs::write(root.join("grants.toml"), "not toml at all [").expect("written");
    let registry = PluginRegistry::new(root, vec![], true);
    let row = registry
        .list()
        .into_iter()
        .find(|row| row.id == "dev.astra.echo")
        .expect("row");
    assert!(row.grants.is_empty(), "a malformed file is no grants");
}

#[test]
fn the_offer_names_everything_and_the_install_persists_the_checked_grants() {
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            // Build and sign a bundle of the fixture plugin in a scratch dir.
            let key = {
                let rng = ring::rand::SystemRandom::new();
                let pkcs8 =
                    ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).expect("generated");
                ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parsed")
            };
            let source = scratch("install-src").join("dev.astra.echo");
            std::fs::create_dir_all(&source).expect("dir");
            std::fs::write(
                source.join("manifest.toml"),
                format!(
                    r#"
id = "dev.astra.echo"
name = "Echo"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.dev.astra.echo.ping"]
permissions = ["read_mailbox", "read_item"]
publisher = "{}"
privacy_policy_url = "https://example.test/privacy"
entrypoint = "echo.exe"
"#,
                    astra_plugin::hex(key.public_key().as_ref())
                ),
            )
            .expect("manifest written");
            std::fs::write(source.join("echo.exe"), b"\x7fbin").expect("binary");
            let bundle = scratch("install-src").join("echo.astra-plugin");
            astra_plugin::pack_and_sign(&source, &bundle, &key).expect("signed");

            let registry = PluginRegistry::new(scratch("install"), vec![], false);
            let offer = registry.install_offer(&bundle).expect("the offer");
            assert_eq!(offer.id, "dev.astra.echo");
            assert_eq!(offer.name, "Echo");
            assert_eq!(offer.permissions, vec!["read_mailbox", "read_item"]);
            assert_eq!(
                offer.privacy_policy_url.as_deref(),
                Some("https://example.test/privacy")
            );
            assert!(!offer.publisher_fingerprint.is_empty());

            registry
                .install(&bundle, &[Permission::ReadMailbox])
                .expect("the install");
            let row = registry
                .list()
                .into_iter()
                .find(|row| row.id == "dev.astra.echo")
                .expect("discovered");
            assert_eq!(row.grants, vec!["read_mailbox".to_string()]);
            assert_eq!(
                row.lifecycle, "discovered",
                "installed, never enabled by the install"
            );
            // The persisted file carries the checked grants for the next boot.
            let reloaded = PluginRegistry::new(scratch("install"), vec![], false);
            let row = reloaded
                .list()
                .into_iter()
                .find(|row| row.id == "dev.astra.echo")
                .expect("row");
            assert_eq!(row.grants, vec!["read_mailbox".to_string()]);
        });
}

#[test]
fn an_unsigned_bundle_is_refused_in_production_and_a_grant_outside_the_manifest_is_refused() {
    crate::runtime::build_runtime()
        .expect("runtime")
        .block_on(|| async {
            let source = scratch("unsigned-src").join("dev.astra.echo");
            std::fs::create_dir_all(&source).expect("dir");
            std::fs::write(
                source.join("manifest.toml"),
                r#"
id = "dev.astra.echo"
name = "Echo"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.dev.astra.echo.ping"]
permissions = ["read_mailbox"]
entrypoint = "echo.exe"
"#,
            )
            .expect("manifest written");
            std::fs::write(source.join("echo.exe"), b"\x7fbin").expect("binary");
            let bundle = scratch("unsigned-src").join("echo.astra-plugin");
            astra_plugin::pack(&source, &bundle).expect("packed");

            let registry = PluginRegistry::new(scratch("unsigned-install"), vec![], false);
            assert!(
                matches!(
                    registry.install_offer(&bundle),
                    Err(RegistryError::Install { .. })
                ),
                "production refuses the unsigned bundle"
            );

            // Dev builds answer the offer, and a grant the manifest never
            // declared refuses at install.
            let dev = PluginRegistry::new(scratch("unsigned-install-dev"), vec![], true);
            assert!(dev.install_offer(&bundle).is_ok());
            assert!(matches!(
                dev.install(&bundle, &[Permission::Send]),
                Err(RegistryError::Install { .. })
            ));
        });
}
