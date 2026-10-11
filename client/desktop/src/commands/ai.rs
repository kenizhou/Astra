// SPDX-License-Identifier: GPL-3.0-only
//! The AI replies half of the command surface: the chips pull the reading
//! pane drives on open and on the draft-pool event, the cached summarise
//! action, the account's style note, the chips dismissal write, and the
//! accepted chip's compose-from-draft publish. Beside them ride the A3
//! assist pulls: the knowledge listing, extraction, and forget, the
//! snippet listing and ranked search, the compose autocomplete, and the
//! cached translate. A child of [`super`], whose command registration
//! this module continues.
//!
//! Split from the command module itself to keep each file under the
//! 500-line limit, the same split the app core's ai_replies made for its
//! drive. The shape follows the reading and resume pulls: thin
//! `#[tauri::command]` wrappers over plain functions the tests drive, the
//! model-backed reads running on the runtime's driver thread the way the
//! resume read and the add flow's dial do, so the webview learns what its
//! pane opens with.

use astra_ai::KnowledgeId;
use astra_app::{
    Chip, ExtractionSummary, KnowledgeView, Snippet, TranslateLanguage, KNOWLEDGE_EXTRACT_LIMIT,
};
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

/// Every knowledge entry the account holds, newest first: the knowledge
/// surface's listing read over the AI bundle's one store. An app with no
/// bundle answers empty.
pub(crate) fn knowledge_list_of(state: &AppState, account: &AccountId) -> Vec<KnowledgeView> {
    state.app.knowledge_list(account)
}

/// Runs one knowledge extraction over the account's most recent mail:
/// `limit` messages at most, the thirty cap when none was named and
/// again when a named limit sits over it (the service's batch guard
/// would drop the surplus anyway, but the window read would spend its
/// body fetches first, so the cap lives here at the boundary). The
/// command returns when the run is done, so the surface's spinner is the
/// only progress there is.
pub(crate) async fn knowledge_extract_of(
    state: &AppState,
    account: &AccountId,
    limit: Option<usize>,
) -> ExtractionSummary {
    let capped = limit.map_or(KNOWLEDGE_EXTRACT_LIMIT, |limit| {
        limit.min(KNOWLEDGE_EXTRACT_LIMIT)
    });
    state.app.knowledge_extract(account, capped).await
}

/// Forgets one knowledge entry by its identity, answering whether any
/// row went: false names an id no entry carried or a store that refused.
pub(crate) fn knowledge_forget_of(state: &AppState, id: &str) -> bool {
    state.app.knowledge_forget(&KnowledgeId::new(id.to_owned()))
}

/// Every snippet, the most recently touched first: the composer picker's
/// plain listing read.
pub(crate) fn snippet_list_of(state: &AppState) -> Vec<Snippet> {
    state.app.snippet_list()
}

/// Searches the snippet library: the ranked answer with its substring
/// fallback, over the whole store listing.
pub(crate) async fn snippet_search_of(state: &AppState, query: &str) -> Vec<Snippet> {
    state.app.snippet_search(query).await
}

/// The compose autocomplete suggestion for the editor's text around the
/// cursor; an absent trailing half crosses as the empty string. An empty
/// answer never shows a chip, and nothing inserts without the user's
/// gesture, which is the editor's own discipline.
pub(crate) async fn compose_autocomplete_of(
    state: &AppState,
    before: &str,
    after: Option<&str>,
) -> String {
    state
        .app
        .compose_autocomplete(before, after.unwrap_or(""))
        .await
}

/// Translates the message's body into `lang`, the cached answer served
/// without a call; an empty answer names a message that cannot be
/// resolved or a call that failed.
pub(crate) async fn translate_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
    lang: TranslateLanguage,
) -> String {
    state.app.translate_message(account, key, lang).await
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

/// Lists every knowledge entry the account holds, newest first; an app
/// with no AI bundle answers empty.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn knowledge_list(state: State<'_, AppState>, account: AccountId) -> Vec<KnowledgeView> {
    knowledge_list_of(&state, &account)
}

/// Runs one knowledge extraction over the account's most recent mail,
/// `limit` messages at most with the thirty cap when none was named and
/// again for a named limit over it, and answers the run's summary once
/// the run is done.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn knowledge_extract(
    state: State<'_, AppState>,
    account: AccountId,
    limit: Option<usize>,
) -> ExtractionSummary {
    state
        .runtime
        .block_on(|| knowledge_extract_of(&state, &account, limit))
}

/// Forgets one knowledge entry, answering whether any row went.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn knowledge_forget(state: State<'_, AppState>, id: String) -> bool {
    knowledge_forget_of(&state, &id)
}

/// Lists every snippet, the most recently touched first.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn snippet_list(state: State<'_, AppState>) -> Vec<Snippet> {
    snippet_list_of(&state)
}

/// Searches the snippet library, the ranked answer with its substring
/// fallback.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn snippet_search(state: State<'_, AppState>, query: String) -> Vec<Snippet> {
    state.runtime.block_on(|| snippet_search_of(&state, &query))
}

/// Answers the compose autocomplete suggestion for the editor's text
/// around the cursor; an empty answer never shows a chip.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn compose_autocomplete(
    state: State<'_, AppState>,
    before: String,
    after: Option<String>,
) -> String {
    state
        .runtime
        .block_on(|| compose_autocomplete_of(&state, &before, after.as_deref()))
}

/// Translates the message's body into the chosen language, the cached
/// answer served without a call.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn translate(
    state: State<'_, AppState>,
    account: AccountId,
    key: ProviderKey,
    lang: TranslateLanguage,
) -> String {
    state
        .runtime
        .block_on(|| translate_of(&state, &account, &key, lang))
}

#[cfg(test)]
#[path = "ai_tests.rs"]
mod tests;
