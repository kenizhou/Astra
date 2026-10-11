// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents half of the command surface: the digest and report
//! generation pulls (one windowed generation each, stored under the
//! window's period), the latest stored document of one kind, and the
//! split inbox's map over the stored categorisation, all riding the
//! app core's own drive. A child of [`super`], whose command
//! registration this module continues, split from the replies module
//! the same way the app core's ai_docs was split from its siblings to
//! keep each file under the 500-line limit.
//!
//! The days clamp lives at this boundary beside the service's own: the
//! core's collection and its prompt builder would each clamp again,
//! but the window read and its body fetches spend first, so the webview
//! never names a window the collection cannot mean (the knowledge
//! extract's limit, the same reasoning).

use astra_ai::AiDocumentKind;
use astra_app::{DigestView, DocumentView, SplitEntry, DOCUMENT_WINDOW_DEFAULT_DAYS};
use engine_api::AccountId;
use tauri::State;

use crate::commands::CommandError;
use crate::AppState;

/// The days a generation window covers once the webview's ask crossed:
/// the default seven when none was named, the named count clamped to
/// the window's own floor and ceiling.
pub(crate) fn window_days(days: Option<u32>) -> u32 {
    days.map_or(DOCUMENT_WINDOW_DEFAULT_DAYS, |days| {
        days.clamp(1, astra_app::DOCUMENT_WINDOW_MAX_DAYS)
    })
}

/// Generates the account's digest over the last `days` days: the
/// core's own collection and one generation, answered as the stored
/// document's view.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when no AI bundle runs, the window
/// cannot be read, the generation fails, or the store refuses the put;
/// no content rides a failure.
pub(crate) async fn digest_generate_of(
    state: &AppState,
    account: &AccountId,
    days: Option<u32>,
) -> Result<DigestView, CommandError> {
    state
        .app
        .generate_digest(account, window_days(days))
        .await
        .map_err(|err| CommandError::Ai(err.to_string()))
}

/// Generates the account's analytics report over the last `days` days:
/// the same window with the Sent count computed in code, answered as
/// the stored document's view.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when no AI bundle runs, the window
/// cannot be read, the generation fails, or the store refuses the put;
/// no content rides a failure.
pub(crate) async fn report_generate_of(
    state: &AppState,
    account: &AccountId,
    days: Option<u32>,
) -> Result<DigestView, CommandError> {
    state
        .app
        .generate_report(account, window_days(days))
        .await
        .map_err(|err| CommandError::Ai(err.to_string()))
}

/// The account's newest stored document of one kind, or `None` when no
/// generation has stored one.
pub(crate) fn latest_document_of(
    state: &AppState,
    account: &AccountId,
    kind: AiDocumentKind,
) -> Option<DocumentView> {
    state.app.latest_document(account, kind)
}

/// The account's split map over its stored categorisation: the pure
/// derivation, no model call at all.
pub(crate) async fn split_map_of(state: &AppState, account: &AccountId) -> Vec<SplitEntry> {
    state.app.split_map(account).await
}

/// Generates the account's digest, storing it under the window's
/// period; `days` defaults to seven and clamps to the window's bounds.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when the generation cannot run; the
/// panel surfaces the sentence.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn digest_generate(
    state: State<'_, AppState>,
    account: AccountId,
    days: Option<u32>,
) -> Result<DigestView, CommandError> {
    state
        .runtime
        .block_on(|| digest_generate_of(&state, &account, days))
}

/// Generates the account's analytics report, storing it under the
/// window's period; `days` defaults to seven and clamps to the window's
/// bounds.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when the generation cannot run; the
/// panel surfaces the sentence.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn report_generate(
    state: State<'_, AppState>,
    account: AccountId,
    days: Option<u32>,
) -> Result<DigestView, CommandError> {
    state
        .runtime
        .block_on(|| report_generate_of(&state, &account, days))
}

/// The account's newest stored document of one kind, or null when no
/// generation has stored one.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn latest_document(
    state: State<'_, AppState>,
    account: AccountId,
    kind: AiDocumentKind,
) -> Option<DocumentView> {
    latest_document_of(&state, &account, kind)
}

/// The account's split map over its stored categorisation: every
/// mapped message's key beside its side, no model call spent.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn split_map(state: State<'_, AppState>, account: AccountId) -> Vec<SplitEntry> {
    state.runtime.block_on(|| split_map_of(&state, &account))
}

#[cfg(test)]
#[path = "ai_docs_tests.rs"]
mod tests;
