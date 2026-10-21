// SPDX-License-Identifier: GPL-3.0-only
//! The plugin command surface: the registry's list / enable / disable /
//! approve verbs. The handlers are thin wrappers over
//! [`crate::plugin::PluginRegistry`]; the logic lives in the registry so the
//! seam is exercised without a webview. No command installs a plugin:
//! discovery is host-side, and approval here is the human's decision arriving
//! from the settings surface.

use tauri::State;

use astra_plugin::Permission;

use super::CommandError;
use crate::{
    plugin::{InstallOffer, PluginRow},
    AppState,
};

/// Lists the discovered plugins.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn plugin_list(state: State<'_, AppState>) -> Vec<PluginRow> {
    state.plugins.list()
}

/// Enables one discovered plugin: loads it and starts its worker.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn plugin_enable(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    state
        .plugins
        .enable(&id)
        .await
        .map_err(|error| CommandError::Intent(error.to_string()))
}

/// Disables one active plugin: its worker is asked to exit and killed
/// regardless.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn plugin_disable(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    state
        .plugins
        .disable(&id)
        .await
        .map_err(|error| CommandError::Intent(error.to_string()))
}

/// Grants one manifest-declared permission to one plugin. The grant applies
/// at the worker's next start.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn plugin_approve(
    state: State<'_, AppState>,
    id: String,
    permission: Permission,
) -> Result<(), CommandError> {
    state
        .plugins
        .approve(&id, permission)
        .map_err(|error| CommandError::Intent(error.to_string()))
}

/// Calls one activated plugin's declared method: the composer actions' and
/// settings fields' whole reach, and the frame bridge's. The answer is the
/// worker's result payload.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn plugin_call(
    state: State<'_, AppState>,
    id: String,
    method: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    state
        .plugins
        .call(&id, &method, args)
        .await
        .map_err(|error| CommandError::Intent(error.to_string()))
}

/// The install offer for one bundle: verified and parsed, nothing unpacked.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn plugin_install_offer(
    state: State<'_, AppState>,
    path: String,
) -> Result<InstallOffer, CommandError> {
    state
        .plugins
        .install_offer(std::path::Path::new(&path))
        .map_err(|error| CommandError::Intent(error.to_string()))
}

/// Installs one bundle with the consented grants; the plugin lands disabled.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn plugin_install(
    state: State<'_, AppState>,
    path: String,
    grants: Vec<Permission>,
) -> Result<(), CommandError> {
    state
        .plugins
        .install(std::path::Path::new(&path), &grants)
        .map_err(|error| CommandError::Intent(error.to_string()))
}
