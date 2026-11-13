// SPDX-License-Identifier: GPL-3.0-only
//! The snooze command surface: the row menu's Snooze verb over the core's
//! own snooze body. A child of [`super`], whose command registration this
//! module continues, split the same way the calendar module was to keep each
//! file under the 500-line limit. The verb names one message and a wake
//! instant; the core records the stable identity and moves the message into
//! the account's Snooze folder.

use astra_app::{MessageRef, RowRef};
use tauri::State;

use crate::AppState;

/// Snoozes one message until `wake_at` (unix seconds): the core records the
/// stable `Message-ID` and the origin folder, then moves the message into
/// the account's Snooze folder. A malformed reference is refused before
/// anything reaches the core.
///
/// # Errors
///
/// Returns the core's refusal sentence (a conversation row, an account or
/// provider gone, a message with no `Message-ID`, a failed move) or the
/// malformed-reference refusal.
pub(crate) async fn snooze_message_of(
    state: &AppState,
    account: &str,
    key: &str,
    wake_at: i64,
) -> Result<(), String> {
    let message = MessageRef::from_parts(account, key.to_owned())
        .ok_or_else(|| "the message reference is malformed".to_owned())?;
    state
        .app
        .snooze_message(RowRef::Message(message), wake_at)
        .await
}

/// Snoozes one message until the wake instant the picker chose: the row
/// menu's single-message verb, the core's two moves behind it.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn snooze_message(
    state: State<'_, AppState>,
    account: String,
    key: String,
    wake_at: i64,
) -> Result<(), String> {
    state
        .runtime
        .block_on(|| snooze_message_of(&state, &account, &key, wake_at))
}

#[cfg(test)]
#[path = "snooze_tests.rs"]
mod tests;
