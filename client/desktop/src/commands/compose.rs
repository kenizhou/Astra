// SPDX-License-Identifier: GPL-3.0-only
//! The compose half of the command surface: the pull for the compose-request
//! snapshot, the prefill a reply or forward opened the composer on. A child
//! of [`super`], whose command registration this module continues, under the
//! same split as the drafts and sending halves.

use tauri::State;

use astra_viewmodel::ComposeRequestSnapshot;

use crate::AppState;

/// The published compose-request snapshot: the composition the last reply or
/// forward opened, or `None` when no request is pending.
pub(crate) fn compose_request_of(state: &AppState) -> Option<ComposeRequestSnapshot> {
    state.app.compose_request()
}

/// Pulls the compose-request snapshot: the prefill a reply or forward opened
/// the composer on, or null when no request is pending, the same
/// None-clears pull reading takes. Compose requests are signalled through on
/// every publish, like reading and draft status, so this is pulled per
/// signal rather than on a debounce.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn compose_request(state: State<'_, AppState>) -> Option<ComposeRequestSnapshot> {
    compose_request_of(&state)
}

/// The consuming take: the pop-out composer window's pull, which hydrates
/// from the answer while the slot empties behind it, so a later window never
/// re-reads a spent request.
pub(crate) fn take_compose_request_of(state: &AppState) -> Option<ComposeRequestSnapshot> {
    state.app.take_compose_request()
}

/// Takes the pending compose request, consuming it: the pop-out composer
/// window's hydration pull. Null when no request is pending, which a window
/// opened on a plain new message reads as its blank composition.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn take_compose_request(state: State<'_, AppState>) -> Option<ComposeRequestSnapshot> {
    take_compose_request_of(&state)
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
