// SPDX-License-Identifier: GPL-3.0-only
//! The plugin registry: the desktop host's managed plugin state.
//!
//! Discovery scans the managed plugins directory (and any
//! `ASTRA_PLUGIN_DEV` development directories the boot hands over) for
//! `manifest.toml` files, parses them with the contract crate's fail-closed
//! parse, and holds one entry per plugin: its manifest, its worker program,
//! its lifecycle, its granted permissions, and its worker host once
//! activated. Nothing here lets a plugin install or activate itself:
//! discovery is host-side, activation is a human decision driven through the
//! command surface, and the grant table starts empty for every plugin. Grants
//! and the enabled flag write through to `grants.toml` beside the root, and
//! the boot restores them; a production build refuses plugins declaring
//! proposed methods.

pub(crate) mod gate;
pub(crate) mod scheme;

mod calls;

mod decode;
mod persist;
pub(crate) use decode::PluginDecoder;
pub(crate) use gate::PluginGate;

#[cfg(test)]
pub(crate) mod testkit;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use astra_plugin::{
    IllegalTransition, Lifecycle, Permission, PluginId, PluginManifest, SendMode, WorkerHost,
};
use serde::Serialize;

/// Why a registry operation failed.
#[derive(Debug)]
pub(crate) enum RegistryError {
    /// No discovered plugin carries this id.
    UnknownPlugin(String),
    /// The plugin is not activated, so there is nothing to disable.
    NotActive(String),
    /// The enable path failed: the worker never reached Activated.
    Enable {
        /// The plugin's id.
        id: String,
        /// What the spawn or the handshake answered.
        reason: String,
    },
    /// The grant asked for a permission the manifest never declared.
    UndeclaredPermission {
        /// The plugin's id.
        id: String,
        /// The permission asked for.
        permission: Permission,
    },
    /// The permission is already granted.
    AlreadyGranted(Permission),
    /// The lifecycle machine refused the transition.
    Lifecycle(IllegalTransition),
    /// A production build refuses to enable a plugin declaring proposed
    /// methods.
    ProposedPlugin(String),
    /// A call into a plugin failed: the worker's refusal or error.
    Call {
        /// The plugin's id.
        id: String,
        /// What the call answered.
        reason: String,
    },
}

impl core::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RegistryError::UnknownPlugin(id) => write!(f, "no discovered plugin {id}"),
            RegistryError::NotActive(id) => write!(f, "plugin {id} is not activated"),
            RegistryError::Enable { id, reason } => {
                write!(f, "plugin {id} failed to enable: {reason}")
            }
            RegistryError::UndeclaredPermission { id, permission } => write!(
                f,
                "plugin {id} never declared permission {}",
                permission.wire_name()
            ),
            RegistryError::AlreadyGranted(permission) => {
                write!(
                    f,
                    "permission {} is already granted",
                    permission.wire_name()
                )
            }
            RegistryError::Lifecycle(error) => write!(f, "{error}"),
            RegistryError::ProposedPlugin(id) => {
                write!(
                    f,
                    "plugin {id} declares proposed methods, which production never enables"
                )
            }
            RegistryError::Call { id, reason } => {
                write!(f, "the call into plugin {id} failed: {reason}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// The handshake deadline one worker's init gets (the plan's 5s).
const INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// The restart budget one worker gets (the plan's 3).
const RESTART_MAX: u32 = 3;

/// The backoff base of the restart schedule (linear, capped at 4s by the
/// supervision machine).
const BACKOFF_BASE: Duration = Duration::from_secs(1);

/// The cap on one UI-driven plugin call (a composer action or a settings
/// field): generous enough for real work, short enough that a stuck worker
/// does not hang a click.
const GATE_CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// One registry row as the command surface reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct PluginRow {
    /// The plugin's id.
    pub(crate) id: String,
    /// Its display name.
    pub(crate) name: String,
    /// Its version.
    pub(crate) version: String,
    /// Its lifecycle state, lowercased (`discovered`, `activated`, ...).
    pub(crate) lifecycle: String,
    /// The permissions the host has granted, as wire names.
    pub(crate) grants: Vec<String>,
    /// The worker program the entry resolves to.
    pub(crate) program: String,
    /// The UI descriptors the manifest declared, when it declared any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ui: Option<astra_plugin::UiDescriptors>,
}

/// One activated plugin serving a method, as the gate consumes it.
pub(crate) struct ServingWorker {
    /// The plugin's id.
    pub(crate) id: String,
    /// Its declared send-gate mode (set for `before_send` servers).
    pub(crate) mode: Option<SendMode>,
    /// Its live worker host.
    pub(crate) host: WorkerHost,
}

/// One discovered plugin.
struct PluginEntry {
    manifest: PluginManifest,
    program: PathBuf,
    /// The plugin's own directory: the frame scheme's only asset root.
    dir: PathBuf,
    lifecycle: Lifecycle,
    grants: Vec<Permission>,
    host: Option<WorkerHost>,
    /// Whether the plugin is enabled; persisted beside the registry root.
    enabled: bool,
}

/// The registry. Cheap to construct; discovery runs on [`PluginRegistry::list`]
/// so a plugin directory dropped in while the app runs appears on the next
/// list without a restart.
pub(crate) struct PluginRegistry {
    root: PathBuf,
    dev_dirs: Vec<PathBuf>,
    dev_build: bool,
    entries: Mutex<HashMap<String, PluginEntry>>,
    /// The persisted grant/enabled records, loaded at construction.
    persisted: std::collections::BTreeMap<String, persist::PluginRecord>,
}

impl PluginRegistry {
    /// A registry over the managed plugins directory `root`, plus any
    /// development plugin directories. `dev_build` gates proposed methods
    /// (the plan's dev-only placeholder tier).
    #[must_use]
    pub(crate) fn new(root: PathBuf, dev_dirs: Vec<PathBuf>, dev_build: bool) -> Self {
        Self {
            persisted: persist::load(&root),
            root,
            dev_dirs,
            dev_build,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Writes the current grant/enabled table through to the grants file.
    fn persist(&self) {
        let records = self
            .entries
            .lock()
            .expect("entries")
            .iter()
            .map(|(id, entry)| {
                (
                    id.clone(),
                    persist::PluginRecord {
                        grants: entry.grants.clone(),
                        enabled: entry.enabled,
                    },
                )
            })
            .collect();
        persist::store(&self.root, &records);
    }

    /// The ids the grants file marks enabled; the boot enables each, and a
    /// worker that now fails lands Failed without ever failing the boot.
    pub(crate) fn restore_enabled(&self) -> Vec<String> {
        self.persisted
            .iter()
            .filter(|(_, record)| record.enabled)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The registry rows, after a fresh discovery pass.
    pub(crate) fn list(&self) -> Vec<PluginRow> {
        self.refresh();
        let mut rows: Vec<PluginRow> = self
            .entries
            .lock()
            .expect("entries")
            .values()
            .map(|entry| PluginRow {
                id: entry.manifest.id.as_str().to_string(),
                name: entry.manifest.name.clone(),
                version: entry.manifest.version.clone(),
                lifecycle: lifecycle_name(&entry.lifecycle).to_string(),
                grants: entry
                    .grants
                    .iter()
                    .map(|grant| grant.wire_name().to_string())
                    .collect(),
                program: entry.program.to_string_lossy().into_owned(),
                ui: entry.manifest.ui.clone(),
            })
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    }

    /// Shuts every running worker down. Called on the exit path, before the
    /// runtime drops (the drop order in `AppState` is load bearing).
    pub(crate) async fn shutdown_all(&self) {
        let hosts: Vec<WorkerHost> = self
            .entries
            .lock()
            .expect("entries")
            .values_mut()
            .filter_map(|entry| entry.host.take())
            .collect();
        for host in hosts {
            host.shutdown().await;
        }
    }

    /// The discovery pass: scan the roots, insert newly seen plugins as
    /// Discovered, and never touch an entry that has moved past discovery.
    fn refresh(&self) {
        for dir in self.candidates() {
            let manifest_path = dir.join("manifest.toml");
            let Ok(text) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            let manifest = match PluginManifest::parse(&text) {
                Ok(manifest) => manifest,
                Err(error) => {
                    log::warn!("plugins: skipping {}: {error}", manifest_path.display());
                    continue;
                }
            };
            let id = manifest.id.as_str().to_string();
            let program = program_of(&dir, &manifest);
            let mut entries = self.entries.lock().expect("entries");
            let record = self.persisted.get(&id);
            entries.entry(id).or_insert(PluginEntry {
                manifest,
                program,
                dir: dir.clone(),
                lifecycle: Lifecycle::Discovered,
                grants: record.map_or_else(Vec::new, |record| record.grants.clone()),
                host: None,
                enabled: record.is_some_and(|record| record.enabled),
            });
        }
    }

    /// Every directory that might hold a manifest: the managed root's
    /// immediate children, plus each dev directory directly.
    fn candidates(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Ok(children) = std::fs::read_dir(&self.root) {
            dirs.extend(
                children
                    .filter_map(std::result::Result::ok)
                    .map(|child| child.path())
                    .filter(|path| path.is_dir()),
            );
        }
        dirs.extend(self.dev_dirs.iter().filter(|dir| dir.is_dir()).cloned());
        dirs
    }

    /// The worker's private data directory for one plugin.
    fn data_dir_of(&self, id: &PluginId) -> PathBuf {
        self.root.join("data").join(id.as_str())
    }
}

/// The worker program a plugin directory resolves to: the manifest's
/// entrypoint when it names one, else the last id segment with the OS
/// executable suffix (`dev.astra.echo` -> `echo.exe` on Windows).
fn program_of(dir: &Path, manifest: &PluginManifest) -> PathBuf {
    if let Some(entrypoint) = &manifest.entrypoint {
        return dir.join(entrypoint);
    }
    let tail = manifest
        .id
        .as_str()
        .rsplit('.')
        .next()
        .expect("a parsed id is never empty");
    dir.join(format!("{tail}{}", std::env::consts::EXE_SUFFIX))
}

/// The lifecycle's lowercase wire name for the row.
fn lifecycle_name(lifecycle: &Lifecycle) -> &'static str {
    match lifecycle {
        Lifecycle::Discovered => "discovered",
        Lifecycle::Loaded => "loaded",
        Lifecycle::Activated => "activated",
        Lifecycle::Failed(_) => "failed",
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
