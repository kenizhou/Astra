// SPDX-License-Identifier: GPL-3.0-only
//! The sending half of the command surface: the pull for the sending
//! snapshot, the queue of every account's sends that have not gone out. A
//! child of [`super`], whose command registration this module continues,
//! split from the command module itself to keep each file under the 500-line
//! limit, like the drafts half before it.

use tauri::State;

use astra_viewmodel::SendingSnapshot;

use crate::AppState;

/// The published sending snapshot: the outbox panel's rows, or nothing
/// outstanding.
pub(crate) fn sending_of(state: &AppState) -> SendingSnapshot {
    state.app.sending()
}

/// Pulls the sending snapshot: the outbox panel's rows, empty when every send
/// has gone. Sending is signalled through on every publish, like reading and
/// draft status, so this is pulled per signal rather than on a debounce.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn sending(state: State<'_, AppState>) -> SendingSnapshot {
    sending_of(&state)
}

#[cfg(test)]
#[path = "sending_tests.rs"]
mod tests;
