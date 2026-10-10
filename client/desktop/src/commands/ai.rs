// SPDX-License-Identifier: GPL-3.0-only
//! The AI replies half of the command surface: the chips pull the reading
//! pane drives on open and on the draft-pool event, the cached summarise
//! action, the account's style note, the chips dismissal write, and the
//! accepted chip's compose-from-draft publish. A child of [`super`], whose
//! command registration this module continues.
//!
//! Split from the command module itself to keep each file under the
//! 500-line limit, the same split the app core's ai_replies made for its
//! drive. The shape follows the reading and resume pulls: thin
//! `#[tauri::command]` wrappers over plain functions the tests drive, the
//! model-backed reads running on the runtime's driver thread the way the
//! resume read and the add flow's dial do, so the webview learns what its
//! pane opens with.

use astra_app::Chip;
use engine_api::{AccountId, ProviderKey};
use tauri::State;

use crate::commands::CommandError;
use crate::AppState;

/// Pulls the message's smart-reply chips: the surfacing gate over the
/// draft pool (the dismissal marker first, then the confidence floor and
/// the binary judge), under a correlation minted for the pull's own turn.
/// An app with no AI bundle answers no chips.
pub(crate) async fn draft_chips_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
) -> Vec<Chip> {
    state.app.draft_chips(account, key).await
}

/// Summarises the message, cached against the body in front of the
/// reader: a hit answers with no model call, a miss runs the Summarise
/// use case once and caches the answer.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when no AI bundle runs, the message
/// cannot be resolved, or the action itself fails; the message is the
/// core's own sentence.
pub(crate) async fn summarise_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
) -> Result<String, CommandError> {
    state
        .app
        .summarise_message(account, key)
        .await
        .map_err(|err| CommandError::Ai(err.to_string()))
}

/// Reads the account's style note, the daily-capped getter the drafting
/// turn reads through: a profile built within the day answers from the
/// store, an absent or stale one is inferred from the account's recent
/// Sent mail. `None` names the no-profile state.
pub(crate) async fn style_profile_of(state: &AppState, account: &AccountId) -> Option<String> {
    state.app.account_style_note(account).await
}

/// Records the reader's dismissal of the message's chips: the marker the
/// surfacing gate consults before it spends a judge call, idempotent by the
/// message identity pair. The write cannot fail the caller: a store
/// the core cannot record into is logged there and the pane hides its
/// chips again on the next pull.
pub(crate) fn chips_dismiss_of(state: &AppState, account: &AccountId, key: &ProviderKey) {
    state.app.dismiss_chips(account, key);
}

/// Publishes the reply composition one accepted chip opens: the reply's own
/// prefill (recipients, subject, threading header, quoted original) with the
/// drafted reply already the document's opening paragraphs, published through
/// the compose-request surface so the composer opens on it exactly as it does
/// on a reply's own dispatch.
pub(crate) async fn compose_from_draft_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
    draft: &str,
) {
    state.app.compose_from_draft(account, key, draft).await;
}

/// Pulls the message's smart-reply chips for the reading pane.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn draft_chips(
    state: State<'_, AppState>,
    account: AccountId,
    key: ProviderKey,
) -> Vec<Chip> {
    state
        .runtime
        .block_on(|| draft_chips_of(&state, &account, &key))
}

/// Summarises the message, cached: a hit answers without a model call.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when the summarise cannot run or fails;
/// the reading pane surfaces the sentence.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn summarise(
    state: State<'_, AppState>,
    account: AccountId,
    key: ProviderKey,
) -> Result<String, CommandError> {
    state
        .runtime
        .block_on(|| summarise_of(&state, &account, &key))
}

/// Reads the account's writing-style note, or null when none was built.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn style_profile(state: State<'_, AppState>, account: AccountId) -> Option<String> {
    state
        .runtime
        .block_on(|| style_profile_of(&state, &account))
}

/// Records the reader's dismissal of the message's chips. The write
/// cannot fail the webview: a store the core cannot record into is logged
/// there, and the pane hides its chips again on the next pull.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn chips_dismiss(state: State<'_, AppState>, account: AccountId, key: ProviderKey) {
    chips_dismiss_of(&state, &account, &key);
}

/// Opens the reply composer on one accepted chip, the drafted reply already
/// the document's opening paragraphs; the composer opens on the published
/// request's own signal. An original the store no longer resolves publishes
/// nothing and the composer stays shut.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn compose_from_draft(
    state: State<'_, AppState>,
    account: AccountId,
    key: ProviderKey,
    reply_text: String,
) {
    state
        .runtime
        .block_on(|| compose_from_draft_of(&state, &account, &key, &reply_text));
}

#[cfg(test)]
#[path = "ai_tests.rs"]
mod tests;
