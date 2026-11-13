// SPDX-License-Identifier: GPL-3.0-only
//! The reminder command surface: the row menu's Remind me verb over the
//! core's own remind body. A child of [`super`], whose command registration
//! this module continues, split the same way the snooze module was to keep
//! each file under the 500-line limit. The verb names one message and a wake
//! instant; the core records the provider key, subject, and sender. The
//! message never moves and is never flagged.

use astra_app::{MessageRef, RowRef};
use tauri::State;

use crate::AppState;

/// Reminds one message at `wake_at` (unix seconds): the core records the
/// provider key, subject, and sender from the engine's own copy. A malformed
/// reference is refused before anything reaches the core.
///
/// # Errors
///
/// Returns the core's refusal sentence (a conversation row, an account or
/// provider gone, a message that no longer resolves) or the
/// malformed-reference refusal.
pub(crate) async fn remind_message_of(
    state: &AppState,
    account: &str,
    key: &str,
    wake_at: i64,
) -> Result<(), String> {
    let message = MessageRef::from_parts(account, key.to_owned())
        .ok_or_else(|| "the message reference is malformed".to_owned())?;
    state
        .app
        .remind_message(RowRef::Message(message), wake_at)
        .await
}

/// Reminds one message at the wake instant the picker chose: the row menu's
/// single-message verb, the core's record behind it (no move, no flag).
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn remind_message(
    state: State<'_, AppState>,
    account: String,
    key: String,
    wake_at: i64,
) -> Result<(), String> {
    state
        .runtime
        .block_on(|| remind_message_of(&state, &account, &key, wake_at))
}

#[cfg(test)]
#[path = "reminders_tests.rs"]
mod tests;
