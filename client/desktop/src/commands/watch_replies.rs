// SPDX-License-Identifier: GPL-3.0-only
//! The watch-replies command surface: the row menu's Watch replies / Unwatch
//! verbs and the watch listing over the core's own watch bodies. A child of
//! [`super`], whose command registration this module continues, split the
//! same way the reminder module was to keep each file under the 500-line
//! limit. A local membership mark: none of these writes the engine. A
//! malformed row reference is refused before anything reaches the core.

use astra_app::RowRef;
use astra_watches::WatchEntry;
use tauri::State;

use crate::AppState;

/// Parses the row reference the webview sent, refusing a malformed one before
/// anything reaches the core.
fn parse_row(row: serde_json::Value) -> Result<RowRef, String> {
    serde_json::from_value(row).map_err(|_| "the row reference is malformed".to_owned())
}

/// Watches the row's conversation, answering whether this call inserted. A
/// malformed row reference is refused before the core.
///
/// # Errors
///
/// Returns the malformed-reference refusal or the core's refusal sentence.
pub(crate) async fn watch_set_of(state: &AppState, row: serde_json::Value) -> Result<bool, String> {
    let row = parse_row(row)?;
    state.app.watch_thread(row).await
}

/// Unwatches the row's conversation, answering whether a row was removed. A
/// malformed row reference is refused before the core.
///
/// # Errors
///
/// Returns the malformed-reference refusal or the core's refusal sentence.
pub(crate) async fn watch_clear_of(
    state: &AppState,
    row: serde_json::Value,
) -> Result<bool, String> {
    let row = parse_row(row)?;
    state.app.unwatch_thread(row).await
}

/// Every watch, insertion order; an app with no store answers empty.
///
/// # Errors
///
/// Returns the core's refusal sentence when the read fails.
pub(crate) async fn watch_list_of(state: &AppState) -> Result<Vec<WatchEntry>, String> {
    state.app.list_watches().await
}

/// Watches the row's conversation: the row menu's Watch replies verb.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn watch_set(
    state: State<'_, AppState>,
    row: serde_json::Value,
) -> Result<bool, String> {
    state.runtime.block_on(|| watch_set_of(&state, row))
}

/// Unwatches the row's conversation: the row menu's Unwatch verb.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn watch_clear(
    state: State<'_, AppState>,
    row: serde_json::Value,
) -> Result<bool, String> {
    state.runtime.block_on(|| watch_clear_of(&state, row))
}

/// Pulls the watch listing the watch-set store seeds from.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn watch_list(state: State<'_, AppState>) -> Result<Vec<WatchEntry>, String> {
    state.runtime.block_on(|| watch_list_of(&state))
}

#[cfg(test)]
#[path = "watch_replies_tests.rs"]
mod tests;
