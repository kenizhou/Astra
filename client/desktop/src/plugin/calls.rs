// SPDX-License-Identifier: GPL-3.0-only
//! The registry's call-time accessors: the worker host lookup, the bundle
//! directory the frame scheme reads, the serving set the gate and decoder
//! fold over, and the one call the plugin surfaces funnel through. Split from
//! the registry's own module to keep it under the repository's 500-line cap;
//! the impl continues the same type.

use std::path::PathBuf;

use astra_plugin::{Lifecycle, Permission, WorkerHost};

use super::{PluginRegistry, RegistryError, ServingWorker, GATE_CALL_TIMEOUT};

impl PluginRegistry {
    /// The worker host of one activated plugin, for the call sites that drive
    /// it directly (the gate's tests shut a worker down through it).
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
