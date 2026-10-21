// SPDX-License-Identifier: GPL-3.0-only
//! The registry's call-time accessors: the worker host lookup, the bundle
//! directory the frame scheme reads, the serving set the gate and decoder
//! fold over, and the one call the plugin surfaces funnel through. Split from
//! the registry's own module to keep it under the repository's 500-line cap;
//! the impl continues the same type.

use std::path::PathBuf;

use astra_plugin::{Lifecycle, Permission, WorkerHost};

use super::{InstallOffer, PluginRegistry, RegistryError, ServingWorker, GATE_CALL_TIMEOUT};
use astra_plugin::{LifecycleEvent, SpawnFn, WorkerConfig};

/// The verbs' supervision knobs, shared with the registry's own file.
use super::{BACKOFF_BASE, INIT_TIMEOUT, RESTART_MAX};

impl PluginRegistry {
    /// Enables one plugin: loads it, spawns its worker, and activates it. A
    /// failure lands the entry in the terminal Failed state with the reason.
    pub(crate) async fn enable(&self, id: &str) -> Result<(), RegistryError> {
        self.enable_inner(id, None).await
    }

    /// Enables one plugin over a custom spawn path: the tests' in-process
    /// workers (the echo plugin over duplex) ride the same lifecycle.
    #[cfg(test)]
    pub(crate) async fn enable_with(&self, id: &str, spawn: SpawnFn) -> Result<(), RegistryError> {
        self.enable_inner(id, Some(spawn)).await
    }

    /// The shared enable: `None` spawns the worker as a real process.
    async fn enable_inner(&self, id: &str, spawn: Option<SpawnFn>) -> Result<(), RegistryError> {
        self.refresh();
        // The production gate: a release build never enables a plugin that
        // declares a proposed method (the dev-only placeholder tier).
        if !self.dev_build {
            let entries = self.entries.lock().expect("entries");
            let entry = entries
                .get(id)
                .ok_or_else(|| RegistryError::UnknownPlugin(id.to_string()))?;
            let proposed = entry
                .manifest
                .methods
                .iter()
                .filter_map(|method| astra_plugin::lookup(method))
                .any(|def| def.stability == astra_plugin::Stability::Proposed);
            if proposed {
                return Err(RegistryError::ProposedPlugin(id.to_string()));
            }
        }
        let (manifest, program, grants) = {
            let mut entries = self.entries.lock().expect("entries");
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| RegistryError::UnknownPlugin(id.to_string()))?;
            if let Lifecycle::Failed(reason) = &entry.lifecycle {
                return Err(RegistryError::Enable {
                    id: id.to_string(),
                    reason: format!("the plugin failed earlier: {reason}"),
                });
            }
            entry
                .lifecycle
                .clone()
                .transition(LifecycleEvent::Load)
                .map_err(RegistryError::Lifecycle)?;
            entry.lifecycle = Lifecycle::Loaded;
            (
                entry.manifest.clone(),
                entry.program.clone(),
                entry.grants.clone(),
            )
        };
        let data_dir = self.data_dir_of(&manifest.id);
        if let Err(error) = std::fs::create_dir_all(&data_dir) {
            let mut entries = self.entries.lock().expect("entries");
            if let Some(entry) = entries.get_mut(id) {
                entry.lifecycle =
                    Lifecycle::Failed(format!("the data directory could not be created: {error}"));
            }
            return Err(RegistryError::Enable {
                id: id.to_string(),
                reason: format!("the data directory could not be created: {error}"),
            });
        }
        let config = WorkerConfig {
            program: program.clone(),
            args: vec![],
            data_dir,
            init_timeout: INIT_TIMEOUT,
            restart_max: RESTART_MAX,
            backoff_base: BACKOFF_BASE,
        };
        let host = match spawn {
            None => WorkerHost::spawn_process(config, manifest, &grants, self.dev_build),
            Some(spawn) => WorkerHost::new(config, manifest, &grants, self.dev_build, spawn),
        };
        let outcome = host.start().await;
        let mut entries = self.entries.lock().expect("entries");
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::UnknownPlugin(id.to_string()))?;
        let answer = match outcome {
            Ok(()) => {
                entry.lifecycle = Lifecycle::Activated;
                entry.host = Some(host);
                entry.enabled = true;
                Ok(())
            }
            Err(error) => {
                let reason = error.to_string();
                entry.lifecycle = Lifecycle::Failed(reason.clone());
                Err(RegistryError::Enable {
                    id: id.to_string(),
                    reason,
                })
            }
        };
        drop(entries);
        self.persist();
        answer
    }

    /// Disables one active plugin: the worker is asked to exit and killed
    /// regardless, and the entry returns to Discovered (a fresh activation is
    /// a fresh instance, per the lifecycle machine's terminal-Failed rule).
    pub(crate) async fn disable(&self, id: &str) -> Result<(), RegistryError> {
        self.refresh();
        let host = {
            let mut entries = self.entries.lock().expect("entries");
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| RegistryError::UnknownPlugin(id.to_string()))?;
            if entry.lifecycle != Lifecycle::Activated {
                return Err(RegistryError::NotActive(id.to_string()));
            }
            entry.lifecycle = Lifecycle::Discovered;
            entry.enabled = false;
            entry.host.take()
        };
        if let Some(host) = host {
            host.shutdown().await;
        }
        self.persist();
        Ok(())
    }

    /// Grants one manifest-declared permission. The grant applies at the
    /// worker's next start; a running worker keeps its init-time grants until
    /// then (the init envelope is the only grant carrier).
    pub(crate) fn approve(&self, id: &str, permission: Permission) -> Result<(), RegistryError> {
        self.refresh();
        let mut entries = self.entries.lock().expect("entries");
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::UnknownPlugin(id.to_string()))?;
        if !entry.manifest.permissions.contains(&permission) {
            return Err(RegistryError::UndeclaredPermission {
                id: id.to_string(),
                permission,
            });
        }
        if entry.grants.contains(&permission) {
            return Err(RegistryError::AlreadyGranted(permission));
        }
        entry.grants.push(permission);
        drop(entries);
        self.persist();
        Ok(())
    }

    /// The worker host of one activated plugin; the tests shut a worker down
    /// through it.
    #[cfg(test)]
    pub(crate) fn worker_host(&self, id: &str) -> Option<WorkerHost> {
        self.entries
            .lock()
            .expect("entries")
            .get(id)
            .and_then(|entry| entry.host.clone())
    }

    /// One discovered plugin's own directory: the `astra-plugin://` scheme's
    /// only asset root for it.
    pub(crate) fn bundle_dir(&self, id: &str) -> Option<PathBuf> {
        self.entries
            .lock()
            .expect("entries")
            .get(id)
            .map(|entry| entry.dir.clone())
    }

    /// Every activated plugin serving `method` with `grant` granted, ready to
    /// be called. The gate and the transform fold over this set; an
    /// unactivated or ungranted plugin is simply not in it.
    pub(crate) fn serving(&self, method: &str, grant: Permission) -> Vec<ServingWorker> {
        if !self.is_active() {
            return Vec::new();
        }
        self.entries
            .lock()
            .expect("entries")
            .values()
            .filter(|entry| {
                entry.lifecycle == Lifecycle::Activated
                    && entry
                        .manifest
                        .methods
                        .iter()
                        .any(|declared| declared == method)
                    && entry.grants.contains(&grant)
            })
            .filter_map(|entry| {
                entry.host.clone().map(|host| ServingWorker {
                    id: entry.manifest.id.as_str().to_string(),
                    mode: entry.manifest.before_send_mode,
                    host,
                })
            })
            .collect()
    }

    /// Whether one plugin holds a grant (the gate's auto-activation check:
    /// `before_send` fires only for admin-granted plugins).
    pub(crate) fn has_grant(&self, id: &str, permission: Permission) -> bool {
        self.entries
            .lock()
            .expect("entries")
            .get(id)
            .is_some_and(|entry| entry.grants.contains(&permission))
    }

    /// Calls one activated plugin's declared method through its worker: the
    /// composer actions' and settings fields' whole reach. The method must be
    /// declared in the manifest (the worker's router enforces the rest), and
    /// the call is capped at the gate's handler timeout.
    pub(crate) async fn call(
        &self,
        id: &str,
        method: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, RegistryError> {
        if !self.is_active() {
            return Err(RegistryError::Disabled);
        }
        let host = {
            let entries = self.entries.lock().expect("entries");
            let entry = entries
                .get(id)
                .ok_or_else(|| RegistryError::UnknownPlugin(id.to_string()))?;
            if entry.lifecycle != Lifecycle::Activated {
                return Err(RegistryError::NotActive(id.to_string()));
            }
            if !entry
                .manifest
                .methods
                .iter()
                .any(|declared| declared == method)
            {
                return Err(RegistryError::Call {
                    id: id.to_string(),
                    reason: format!("method {method} is not declared in the manifest"),
                });
            }
            entry.host.clone()
        };
        let host = host.ok_or_else(|| RegistryError::NotActive(id.to_string()))?;
        tokio::time::timeout(GATE_CALL_TIMEOUT, host.call(None, method, args))
            .await
            .map_err(|_| RegistryError::Call {
                id: id.to_string(),
                reason: "the plugin did not answer in time".to_string(),
            })?
            .map_err(|error| RegistryError::Call {
                id: id.to_string(),
                reason: error.to_string(),
            })
    }
}

impl PluginRegistry {
    /// The install offer for one bundle: verified (signed bundles always;
    /// unsigned only in dev builds), parsed, and named for the consent
    /// dialog. Nothing is unpacked or registered here.
    pub(crate) fn install_offer(
        &self,
        path: &std::path::Path,
    ) -> Result<InstallOffer, RegistryError> {
        let verified = astra_plugin::verify(path, !self.dev_build).map_err(|error| {
            RegistryError::Install {
                reason: error.to_string(),
            }
        })?;
        Ok(InstallOffer {
            id: verified.manifest.id.as_str().to_string(),
            name: verified.manifest.name.clone(),
            version: verified.manifest.version.clone(),
            publisher_fingerprint: verified.publisher_fingerprint,
            permissions: verified
                .manifest
                .permissions
                .iter()
                .map(|permission| permission.wire_name().to_string())
                .collect(),
            privacy_policy_url: verified.manifest.privacy_policy_url.clone(),
        })
    }

    /// Installs one bundle: re-verified (the offer computes nothing the
    /// install does not re-check), unpacked into the managed directory,
    /// the consented grants recorded and persisted, and the plugin left
    /// disabled until the person enables it. A grant the manifest never
    /// declared refuses the whole install.
    pub(crate) fn install(
        &self,
        path: &std::path::Path,
        grants: &[Permission],
    ) -> Result<(), RegistryError> {
        let verified = astra_plugin::verify(path, !self.dev_build).map_err(|error| {
            RegistryError::Install {
                reason: error.to_string(),
            }
        })?;
        for grant in grants {
            if !verified.manifest.permissions.contains(grant) {
                return Err(RegistryError::Install {
                    reason: format!(
                        "the consent grants {}, which the manifest never declared",
                        grant.wire_name()
                    ),
                });
            }
        }
        astra_plugin::verify_and_unpack(path, &self.root, !self.dev_build).map_err(|error| {
            RegistryError::Install {
                reason: error.to_string(),
            }
        })?;
        self.refresh();
        let id = verified.manifest.id.as_str().to_string();
        {
            let mut entries = self.entries.lock().expect("entries");
            if let Some(entry) = entries.get_mut(&id) {
                entry.grants = grants.to_vec();
                entry.enabled = false;
            }
        }
        self.persist();
        Ok(())
    }
}
