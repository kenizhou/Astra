// SPDX-License-Identifier: GPL-3.0-only
//! The agent sidecar host: the relay that spawns and supervises the Node
//! process, the permission bridge that routes its asks to the UI and the
//! answers back, and the frontend event seam everything streams through.
//!
//! Ported from the kylins agent module, trimmed to SPA0's chat: no
//! conversation mirror, no background drafting, no compaction. The relay
//! owns the process; [`supervisor`] owns the restart policy; the commands
//! live in [`commands`].

// The command surface is declared `pub(crate)` only so the shell's invoke
// handler can name its handlers; its logic is the crate's own.
pub(crate) mod commands;
mod node;
mod permissions;
mod protocol;
mod relay;
mod supervisor;

#[cfg(test)]
#[path = "commands_tests.rs"]
mod commands_tests;

#[cfg(test)]
#[path = "relay_tests.rs"]
mod relay_tests;

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod supervisor_tests;

#[cfg(test)]
#[path = "testkit.rs"]
pub(crate) mod testkit;

pub(crate) use commands::chat_model_config;
pub(crate) use node::spawn_sidecar;
pub(crate) use permissions::PendingApproval;
pub(crate) use protocol::ModelConfig;
pub(crate) use relay::SidecarRelay;

use std::sync::Arc;

use astra_ai::AiStore;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// The frontend-facing stream payload: the wire's event shapes plus the
/// turn markers the host itself adds when a prompt settles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StreamPayload {
    /// A chunk of assistant text.
    TextDelta {
        /// The text chunk.
        text: String,
    },
    /// The model invoked a tool.
    ToolCall {
        /// The tool's name.
        name: String,
        /// The tool's arguments.
        args: serde_json::Value,
        /// The call's id, for pairing with its result.
        tool_call_id: String,
    },
    /// A harness state word, for example `thinking`.
    State {
        /// The state word.
        state: String,
    },
    /// A tool call finished.
    ToolResult {
        /// The finished call's id.
        tool_call_id: String,
        /// The tool's name.
        name: String,
        /// How the call ended.
        status: String,
        /// The tool's output, when it produced one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
    /// A prompt settled; the last event of its turn.
    Done {
        /// The prompt that settled.
        prompt_id: String,
    },
    /// A turn failed.
    Error {
        /// The failure.
        message: String,
        /// The prompt that failed, when one was in flight.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt_id: Option<String>,
    },
}

impl From<&protocol::AgentEvent> for StreamPayload {
    fn from(event: &protocol::AgentEvent) -> Self {
        match event.clone() {
            protocol::AgentEvent::TextDelta { text } => Self::TextDelta { text },
            protocol::AgentEvent::ToolCall {
                name,
                args,
                tool_call_id,
            } => Self::ToolCall {
                name,
                args,
                tool_call_id,
            },
            protocol::AgentEvent::State { state } => Self::State { state },
            protocol::AgentEvent::ToolResult {
                tool_call_id,
                name,
                status,
                output,
            } => Self::ToolResult {
                tool_call_id,
                name,
                status,
                output,
            },
        }
    }
}

/// One streamed turn event for the UI: the session it belongs to, the
/// turn's correlation id, and the payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct AgentStreamEvent {
    /// The session the event belongs to.
    pub(crate) session_id: String,
    /// The turn's correlation id, when the event belongs to one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) correlation: Option<String>,
    /// The payload.
    pub(crate) event: StreamPayload,
}

/// A pending approval settled: by the UI's answer or by a fail close.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PermissionResolved {
    /// The request that settled.
    pub(crate) id: String,
    /// Whether the user approved.
    pub(crate) approved: bool,
    /// Whether the approval was set to persist for the tool.
    pub(crate) always_allow: bool,
}

/// The frontend event seam: everything the agent subsystem tells the UI.
/// Production emits through the Tauri event system; tests collect.
pub(crate) trait AgentEventSink: Send + Sync {
    /// One streamed turn event (`agent:event`).
    fn emit_agent_event(&self, event: AgentStreamEvent);
    /// The init handshake completed (`agent:sidecar-ready`).
    fn emit_sidecar_ready(&self, models: Vec<String>, harnesses: Vec<String>);
    /// The restart budget was exhausted (`agent:sidecar-lost`).
    fn emit_sidecar_lost(&self, attempts: u32, message: &str);
    /// A tool call needs the user's decision (`agent:permission-requested`).
    fn emit_permission_request(&self, approval: PendingApproval);
    /// A pending approval settled (`agent:permission-resolved`).
    fn emit_permission_resolved(&self, resolved: PermissionResolved);
}

/// The production sink: emits the agent events to the main window.
pub(crate) struct TauriAgentSink(AppHandle);

impl TauriAgentSink {
    /// Wraps the app handle the boot resolved.
    pub(crate) fn new(handle: AppHandle) -> Self {
        Self(handle)
    }
}

impl AgentEventSink for TauriAgentSink {
    fn emit_agent_event(&self, event: AgentStreamEvent) {
        if let Err(e) = self.0.emit("agent:event", &event) {
            log::warn!("agent: the agent:event emit failed: {e}");
        }
    }

    fn emit_sidecar_ready(&self, models: Vec<String>, harnesses: Vec<String>) {
        let payload = serde_json::json!({ "models": models, "harnesses": harnesses });
        if let Err(e) = self.0.emit("agent:sidecar-ready", &payload) {
            log::warn!("agent: the agent:sidecar-ready emit failed: {e}");
        }
    }

    fn emit_sidecar_lost(&self, attempts: u32, message: &str) {
        let payload = serde_json::json!({ "attempts": attempts, "message": message });
        if let Err(e) = self.0.emit("agent:sidecar-lost", &payload) {
            log::warn!("agent: the agent:sidecar-lost emit failed: {e}");
        }
    }

    fn emit_permission_request(&self, approval: PendingApproval) {
        if let Err(e) = self.0.emit("agent:permission-requested", &approval) {
            log::warn!("agent: the agent:permission-requested emit failed: {e}");
        }
    }

    fn emit_permission_resolved(&self, resolved: PermissionResolved) {
        if let Err(e) = self.0.emit("agent:permission-resolved", &resolved) {
            log::warn!("agent: the agent:permission-resolved emit failed: {e}");
        }
    }
}

/// The headless sink: narrates the agent events to the log, where the
/// headless slice runner already sends its progress.
pub(crate) struct LogAgentSink;

impl AgentEventSink for LogAgentSink {
    fn emit_agent_event(&self, event: AgentStreamEvent) {
        log::debug!("agent event: {event:?}");
    }

    fn emit_sidecar_ready(&self, models: Vec<String>, _harnesses: Vec<String>) {
        log::info!("agent: the sidecar is ready with {} model(s)", models.len());
    }

    fn emit_sidecar_lost(&self, attempts: u32, message: &str) {
        log::warn!("agent: the sidecar was lost after {attempts} attempt(s): {message}");
    }

    fn emit_permission_request(&self, approval: PendingApproval) {
        log::info!("agent: {} needs a decision", approval.tool);
    }

    fn emit_permission_resolved(&self, resolved: PermissionResolved) {
        log::info!("agent: the {} approval settled", resolved.id);
    }
}

/// The restart budget and the backoff base the shipped host runs with: a
/// linear backoff of one, two, then three seconds, capped at four, and a
/// Lost state after three consecutive failed restarts.
pub(crate) const DEFAULT_RESTART_MAX: u32 = 3;
/// The backoff base in milliseconds.
pub(crate) const DEFAULT_BACKOFF_BASE_MS: u64 = 1000;

/// Builds the relay over the resolved runtime pieces: the node binary, the
/// bundle script, the data directory, the model config when a chat tier is
/// configured, and the store the ledgers land in. Resolution failures are
/// carried as the config error the first start reports, never a panic: the
/// host keeps running without the agent.
pub(crate) fn build_relay(
    data_dir: &std::path::Path,
    model: Option<ModelConfig>,
    sink: Arc<dyn AgentEventSink>,
    store: Arc<AiStore>,
) -> Arc<SidecarRelay> {
    let mut config_error = None;
    let mut node_path = std::path::PathBuf::new();
    let mut script = std::path::PathBuf::new();
    match (node::find_node(), node::resolve_sidecar_script()) {
        (Ok(node), Ok(resolved)) => {
            node_path = node;
            script = resolved;
        }
        (Err(e), _) | (_, Err(e)) => config_error = Some(e),
    }
    let cli_path = node::resolve_cli_path();
    let harness = std::env::var("ASTRA_AGENT_HARNESS").unwrap_or_else(|_| "pi".to_string());
    let mock_fixture = std::env::var("ASTRA_AGENT_MOCK_FIXTURE").ok();
    let mut model_configs = std::collections::BTreeMap::new();
    if let Some(model) = model {
        model_configs.insert("chat".to_string(), model);
    }
    let spawn = spawn_sidecar(
        node_path.clone(),
        script.clone(),
        data_dir.to_owned(),
        Vec::new(),
    );
    SidecarRelay::new(
        relay::SidecarConfig {
            node_path,
            script,
            data_dir: data_dir.to_owned(),
            cli_path,
            harness,
            mock_fixture,
            model_configs,
            policy: default_policy(),
            restart_max: DEFAULT_RESTART_MAX,
            backoff_base_ms: DEFAULT_BACKOFF_BASE_MS,
            config_error,
            spawn,
        },
        sink,
        store,
    )
}

/// The permission policy SPA0 ships: the read verbs run, the two draft
/// verbs ask, and anything unknown fails closed inside the sidecar's
/// bridge (an absent row means needs-approval there).
pub(crate) fn default_policy() -> std::collections::BTreeMap<String, protocol::PermissionPolicy> {
    let auto = || protocol::PermissionPolicy {
        level: protocol::PermissionLevel::Auto,
        allowlist: None,
    };
    let confirm = || protocol::PermissionPolicy {
        level: protocol::PermissionLevel::Confirm,
        allowlist: None,
    };
    [
        ("list_folders".to_string(), auto()),
        ("list_messages".to_string(), auto()),
        ("read_message".to_string(), auto()),
        ("save_draft".to_string(), confirm()),
        ("discard_draft".to_string(), confirm()),
    ]
    .into_iter()
    .collect()
}
