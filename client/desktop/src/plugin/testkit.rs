// SPDX-License-Identifier: GPL-3.0-only
//! The shared test kit for the plugin module's tests: the duplex spawn path
//! every test's scripted worker or in-process echo rides, and the registry
//! builder that writes a manifest, approves the grants, and enables the
//! plugin over it.

use std::path::PathBuf;
use std::sync::Arc;

use astra_plugin::{Permission, SpawnFn, WorkerPipes};
use tokio::io::DuplexStream;

use super::PluginRegistry;

/// A spawn function driving one worker loop over a duplex pair.
pub(crate) fn spawn_of(
    worker: impl Fn(DuplexStream) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
) -> SpawnFn {
    Arc::new(move || {
        let (host_end, worker_end) = tokio::io::duplex(4096);
        let (read, write) = tokio::io::split(host_end);
        worker(worker_end);
        Box::pin(async move {
            Ok(WorkerPipes {
                reader: Box::new(tokio::io::BufReader::new(read)),
                writer: Box::new(write),
                killer: Box::new(|| Box::pin(async {})),
            })
        })
    })
}

/// The echo plugin over a duplex pair: the conformance counterparty itself.
pub(crate) fn spawn_echo() -> SpawnFn {
    spawn_of(|stream| {
        let (read, write) = tokio::io::split(stream);
        tokio::spawn(async move {
            let _ = astra_plugin_echo::run(tokio::io::BufReader::new(read), write).await;
        })
    })
}

pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "astra-desktop-plugin-{}-{}",
        name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A registry holding one plugin, its manifest written under the scratch root
/// with every permission the methods need declared, the given grants approved,
/// enabled over the given spawn path.
pub(crate) async fn registry_with(
    name: &str,
    id: &str,
    methods: &[&str],
    mode: Option<&str>,
    grants: &[Permission],
    spawn: SpawnFn,
) -> Arc<PluginRegistry> {
    let root = scratch(name);
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).expect("plugin dir");
    let methods_toml = methods
        .iter()
        .map(|method| format!("\"{method}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mode_toml = mode
        .map(|mode| format!("before_send_mode = \"{mode}\"\n"))
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
methods = [{methods_toml}]
permissions = ["read_item", "read_mailbox", "compose_edit", "send", "admin"]
{mode_toml}"#
        ),
    )
    .expect("manifest written");
    let registry = Arc::new(PluginRegistry::new(root, vec![], true));
    registry.list();
    for grant in grants {
        registry.approve(id, *grant).expect("declared grant");
    }
    registry
        .enable_with(id, spawn)
        .await
        .expect("the plugin enables");
    registry
}
