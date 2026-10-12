// SPDX-License-Identifier: GPL-3.0-only
//! The MCP listener host: the run/stop seam the advanced-settings panel
//! drives. Enabling binds the endpoint derived from the app's data
//! directory and serves it on a runtime of this host's own (the mail
//! runtime stays the mail runtime's); disabling aborts the accept task,
//! which drops the listener and closes the endpoint. A taken endpoint
//! name is the failed state the panel reads, never a crash: the host
//! answers every seam call whatever happened to the bind.

use std::path::Path;
use std::sync::{Arc, Mutex};

use astra_mcp::server::Server;
use astra_mcp::transport::{Endpoint, Listener};

/// The listener's state as the settings panel reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum McpStatus {
    /// The enabled flag is off: nothing listens.
    Off,
    /// The listener is serving on the endpoint.
    Listening,
    /// The endpoint could not be bound; the string names why for the
    /// panel.
    Failed(String),
}

/// What a stopped-or-never-started host holds, and what a running one
/// holds: the accept task is the listener's lifetime (aborting it drops
/// the `Listener`, which closes the endpoint and, on Unix, unlinks the
/// socket).
enum Run {
    /// Nothing is listening.
    Down,
    /// The accept loop is serving.
    Up {
        /// The spawned accept task; aborting it stops the listener.
        task: tokio::task::JoinHandle<()>,
    },
}

/// The listener's run/stop seam.
pub(crate) struct McpHost {
    /// The server the accept loop serves.
    server: Arc<Server>,
    /// The endpoint derived from the app's data directory.
    endpoint: Endpoint,
    /// A runtime of this host's own: the accept loop never shares the
    /// mail runtime, so a slow or hostile MCP client can hold it without
    /// holding mail.
    runtime: crate::runtime::AppRuntime,
    /// The run state under the host's own lock.
    run: Mutex<Run>,
    /// The panel-facing state: what `status` answers. Separate from `run`
    /// because a failed bind is a state without a task.
    status: Mutex<McpStatus>,
}

impl McpHost {
    /// Builds the host over `server`, deriving the endpoint from
    /// `data_dir` and giving the listener a runtime of its own, so a slow
    /// or hostile MCP client can hold it without holding mail.
    pub(crate) fn new(server: Arc<Server>, data_dir: &Path) -> Self {
        let runtime = crate::runtime::build_runtime().expect("the listener runtime builds");
        Self {
            server,
            endpoint: Endpoint::for_data_dir(data_dir),
            runtime,
            run: Mutex::new(Run::Down),
            status: Mutex::new(McpStatus::Off),
        }
    }

    /// The endpoint, for the panel's client-config snippet.
    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// The listener's state.
    pub(crate) fn status(&self) -> McpStatus {
        self.status.lock().expect("mcp status lock").clone()
    }

    /// Sets the allowed account ids: the gate a live connection answers
    /// through, read fresh on every call. Session state only: nothing
    /// persists, so every boot starts with the gate empty.
    pub(crate) fn set_allowed(&self, ids: impl IntoIterator<Item = String>) {
        self.server.gate().set_allowed(ids);
    }

    /// The allowed account ids, in stable order: the picker's checked
    /// set as the panel's pull reads it back.
    pub(crate) fn allowed(&self) -> Vec<String> {
        self.server.gate().allowed()
    }

    /// Turns the listener on or off: on binds the endpoint and serves it;
    /// off aborts the accept task, which closes the endpoint. Binding a
    /// name another listener owns is the failed state, and a later on
    /// after that holder goes retries the bind honestly.
    pub(crate) fn set_enabled(&self, on: bool) {
        let mut run = self.run.lock().expect("mcp run lock");
        let mut status = self.status.lock().expect("mcp status lock");
        if on {
            if matches!(*run, Run::Up { .. }) {
                return;
            }
            // The bind creates the pipe through tokio, so it runs on the
            // host's own runtime, the house block_on the one sanctioned
            // synchronous wait on it.
            let endpoint = self.endpoint.clone();
            let bound = self.runtime.block_on(|| {
                let endpoint = endpoint.clone();
                async move { Listener::bind(&endpoint) }
            });
            match bound {
                Ok(listener) => {
                    let server = Arc::clone(&self.server);
                    let task = self.runtime.handle().spawn(async move {
                        listener.accept(server).await;
                    });
                    *run = Run::Up { task };
                    *status = McpStatus::Listening;
                }
                Err(err) => {
                    *run = Run::Down;
                    *status = McpStatus::Failed(err.to_string());
                }
            }
        } else {
            if let Run::Up { task } = std::mem::replace(&mut *run, Run::Down) {
                task.abort();
            }
            *status = McpStatus::Off;
        }
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
