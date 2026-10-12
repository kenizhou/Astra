// SPDX-License-Identifier: GPL-3.0-only
//! The MCP half of the command surface: the advanced panel's status
//! pull and its two mutations. The mcp state is HOST-side, never a
//! preference: the [`McpHost`] owns the listener, its runtime, and the
//! gate, and the app core has no path back to it (the host builds the
//! server over the core, never the other way round). So these are host
//! commands over the assembled host, the agent relay command pattern,
//! and no intent or settings-block field is involved: nothing of the
//! mcp state persists, and every boot starts off and empty.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::mcp::McpStatus;

/// The listener's state word, as the panel reads it; crosses as a
/// snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpStateWord {
    /// The enabled flag is off: nothing listens.
    Off,
    /// The listener is serving the endpoint.
    Listening,
    /// The bind failed; the panel shows the diagnostic.
    Failed,
}

/// The advanced panel's pull: the state word, the bind diagnostic when
/// the bind failed, the endpoint string, the gate's allowed account
/// ids, and the relay argv a client config pastes. One pull for the
/// whole panel, so the toggle, the picker, and the snippet cannot
/// disagree about what stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct McpStatusView {
    /// The listener's state word.
    pub(crate) status: McpStateWord,
    /// Why the bind failed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    /// The endpoint as the string the relay argv carries.
    pub(crate) endpoint: String,
    /// The account ids the gate allows, in stable order.
    pub(crate) allowed: Vec<String>,
    /// The relay command: the binary beside the app, the endpoint as
    /// its argument.
    pub(crate) relay_command: Vec<String>,
}

/// The relay's binary name beside the app: the relay crate's own
/// binary, with the extension Windows carries.
const RELAY_BINARY: &str = if cfg!(windows) {
    "astra-mcp-relay.exe"
} else {
    "astra-mcp-relay"
};

/// The client-config snippet's command: the relay binary beside the
/// running app, the endpoint as its argument, exactly the pair the
/// relay's own usage line spells. Pure over its inputs, so the
/// derivation is testable without a running app.
pub(crate) fn relay_command(exe: &Path, endpoint: &str) -> Vec<String> {
    vec![
        exe.with_file_name(RELAY_BINARY)
            .to_string_lossy()
            .into_owned(),
        "--endpoint".to_owned(),
        endpoint.to_owned(),
    ]
}

/// The panel's pull, over the host the boot assembled.
pub(crate) fn mcp_status_of(state: &AppState) -> McpStatusView {
    let host = &state.mcp;
    let (status, reason) = match host.status() {
        McpStatus::Off => (McpStateWord::Off, None),
        McpStatus::Listening => (McpStateWord::Listening, None),
        McpStatus::Failed(reason) => (McpStateWord::Failed, Some(reason)),
    };
    let endpoint = host.endpoint().as_str().to_owned();
    // The snippet names the relay beside the running app; an unreadable
    // exe path falls back to the bare binary name, which the relay's
    // own usage line still explains.
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(RELAY_BINARY));
    McpStatusView {
        status,
        reason,
        allowed: host.allowed(),
        relay_command: relay_command(&exe, &endpoint),
        endpoint,
    }
}

/// Turns the listener on or off: the host's run/stop seam, synchronous
/// and infallible (a failed bind is the failed STATE the pull reads,
/// never an error return).
pub(crate) fn mcp_set_enabled_of(state: &AppState, on: bool) {
    state.mcp.set_enabled(on);
}

/// Replaces the allowed account ids: the gate a live connection answers
/// through, read fresh on every call.
pub(crate) fn mcp_set_allowed_of(state: &AppState, accounts: Vec<String>) {
    state.mcp.set_allowed(accounts);
}

/// Pulls the panel's status view.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn mcp_status(state: State<'_, AppState>) -> McpStatusView {
    mcp_status_of(&state)
}

/// Turns the MCP listener on or off.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn mcp_set_enabled(state: State<'_, AppState>, on: bool) {
    mcp_set_enabled_of(&state, on);
}

/// Replaces the MCP account allowlist.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn mcp_set_allowed(state: State<'_, AppState>, accounts: Vec<String>) {
    mcp_set_allowed_of(&state, accounts);
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
