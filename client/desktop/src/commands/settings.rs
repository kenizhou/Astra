// SPDX-License-Identifier: GPL-3.0-only
//! The settings half of the command surface: the settings snapshot pull.
//! A child of [`super`], whose command registration this module continues,
//! split the same way the calendar and people halves were to keep each
//! file under the 500-line limit. The pull mirrors the mailbox-list pair;
//! every mutation rides the intent dispatch instead, the established
//! architecture, so this module owns no verb of its own.

use astra_viewmodel::SettingsSnapshot;
use tauri::State;

use crate::AppState;

/// The published settings snapshot: the persisted settings block, one row
/// per configured account, and the about block.
pub(crate) fn settings_of(state: &AppState) -> SettingsSnapshot {
    state.app.settings()
}

/// Pulls the settings snapshot.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn settings(state: State<'_, AppState>) -> SettingsSnapshot {
    settings_of(&state)
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
