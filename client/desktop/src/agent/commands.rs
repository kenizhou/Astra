// SPDX-License-Identifier: GPL-3.0-only
//! The agent command surface: thin Tauri shells over the relay and the
//! chat store, following the host's inner-function convention so the
//! tests drive the same seam the webview invokes. [`agent_prompt_with`]
//! generates the turn's correlation id when the caller brought none, and
//! the id echoes on every event, ledger row, and audit row of the turn.
//! The chat commands wrap the store reads and writes of
//! [`crate::agent::chats`]: the rows the panel's thread list paints from.

use astra_ai::{ChatMessageRow, ChatRow, CorrelationId, LlmUseCase};
use tauri::State;

use super::chats;
use crate::{app_state::AppState, commands::CommandError, LlmClient};

/// The relay status as the panel reads it: the state word and the model
/// the chat tier resolved to, when it resolved to one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AgentStatusView {
    /// The relay's lifecycle word: `down`, `starting`, `ready`,
    /// `restarting`, or `lost`.
    pub(crate) state: String,
    /// The model the chat tier's primary endpoint names, when a chat tier
    /// is configured.
    pub(crate) model: Option<String>,
}

/// What one accepted prompt answers: the id its events carry and the chat
/// its turn records into. A prompt without a chat id mints one, so the
/// panel adopts the receipt's chat and keeps riding it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AgentPromptReceipt {
    /// The prompt id every event of the turn names.
    pub(crate) prompt_id: String,
    /// The chat the turn records into (the minted id when the caller
    /// brought none).
    pub(crate) chat_id: String,
}

/// One loaded thread as the panel reads it: the chat's row beside its
/// messages, oldest first.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ChatLoadView {
    /// The chat's own row.
    pub(crate) chat: ChatRow,
    /// The chat's message rows, oldest first: the one source of truth the
    /// visible log paints from.
    pub(crate) messages: Vec<ChatMessageRow>,
}

/// Sends one prompt and returns its receipt. The correlation id is the
/// caller's when it brought one and a fresh uuid otherwise; either way it
/// rides the prompt envelope and echoes on every event of the turn. A
/// caller-named chat id is validated (a deleted chat refuses); an absent
/// one mints a chat titled from the prompt's first line.
///
/// # Errors
/// Returns [`CommandError::Agent`] when the named chat is unknown, the
/// sidecar cannot be started, or the prompt envelope cannot be written.
pub(crate) async fn agent_prompt_with(
    state: &AppState,
    prompt: &str,
    correlation: Option<String>,
    chat_id: Option<String>,
) -> Result<AgentPromptReceipt, CommandError> {
    let chat = chats::resolve_chat(&state.ai_store, chat_id.as_deref(), prompt)?;
    let correlation = correlation.map(CorrelationId::new);
    let prompt_id = state
        .agent
        .prompt(prompt, correlation, Some(&chat))
        .await
        .map_err(CommandError::Agent)?;
    Ok(AgentPromptReceipt {
        prompt_id,
        chat_id: chat.to_string(),
    })
}

/// Every chat, the most recently touched first: the panel's thread list.
///
/// # Errors
/// Returns [`CommandError::Agent`] when the store read fails.
pub(crate) fn chat_list_with(state: &AppState) -> Result<Vec<ChatRow>, CommandError> {
    chats::chat_list_of(&state.ai_store)
}

/// One chat's messages, oldest first: the rows a loaded thread paints
/// from. An unknown chat answers empty (a thread with no turns).
///
/// # Errors
/// Returns [`CommandError::Agent`] when the store read fails.
pub(crate) fn chat_load_with(
    state: &AppState,
    chat_id: &str,
) -> Result<ChatLoadView, CommandError> {
    let messages = chats::chat_messages_of(&state.ai_store, chat_id)?;
    let chat = chats::chat_list_of(&state.ai_store)?
        .into_iter()
        .find(|row| row.id.as_str() == chat_id)
        .ok_or_else(|| CommandError::Agent(format!("no chat is stored under {chat_id}")))?;
    Ok(ChatLoadView { chat, messages })
}

/// Rewrites one chat's title.
///
/// # Errors
/// Returns [`CommandError::Agent`] when the store write fails.
pub(crate) fn chat_rename_with(
    state: &AppState,
    chat_id: &str,
    title: &str,
) -> Result<bool, CommandError> {
    chats::chat_rename_of(&state.ai_store, chat_id, title)
}

/// Deletes one chat together with its messages.
///
/// # Errors
/// Returns [`CommandError::Agent`] when the store write fails.
pub(crate) fn chat_delete_with(state: &AppState, chat_id: &str) -> Result<bool, CommandError> {
    chats::chat_delete_of(&state.ai_store, chat_id)
}

/// Cancels the prompt: best effort on the wire, and every approval the
/// turn suspended is fail-closed on the way out.
///
/// # Errors
/// Returns [`CommandError::Agent`] when the relay is gone.
pub(crate) async fn agent_abort_with(
    state: &AppState,
    prompt_id: &str,
) -> Result<(), CommandError> {
    state.agent.abort(prompt_id).await;
    Ok(())
}

/// The relay status: the lifecycle word plus the configured chat model.
pub(crate) fn agent_status_of(state: &AppState) -> AgentStatusView {
    AgentStatusView {
        state: state.agent.status().as_str().to_owned(),
        model: state
            .llm
            .as_ref()
            .and_then(|client| client.primary_endpoint(LlmUseCase::AssistantChat))
            .map(|endpoint| endpoint.model),
    }
}

/// The UI's answer to one pending approval.
///
/// # Errors
/// Returns [`CommandError::Agent`] when nothing is pending under the id or
/// the response cannot be delivered.
pub(crate) async fn permission_response_with(
    state: &AppState,
    id: &str,
    approved: bool,
    always_allow: bool,
) -> Result<(), CommandError> {
    state
        .agent
        .respond_permission(id, approved, always_allow)
        .await
        .map_err(CommandError::Agent)
}

/// Sends one prompt to the agent, returning the receipt its events and
/// its chat carry.
///
/// # Errors
/// As [`agent_prompt_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn agent_prompt(
    state: State<'_, AppState>,
    prompt: String,
    correlation: Option<String>,
    chat_id: Option<String>,
) -> Result<AgentPromptReceipt, CommandError> {
    agent_prompt_with(&state, &prompt, correlation, chat_id).await
}

/// Cancels the agent's current prompt.
///
/// # Errors
/// As [`agent_abort_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn agent_abort(
    state: State<'_, AppState>,
    prompt_id: String,
) -> Result<(), CommandError> {
    agent_abort_with(&state, &prompt_id).await
}

/// Pulls the agent status.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn agent_status(state: State<'_, AppState>) -> AgentStatusView {
    agent_status_of(&state)
}

/// Answers one pending agent approval.
///
/// # Errors
/// As [`permission_response_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn permission_response(
    state: State<'_, AppState>,
    id: String,
    approved: bool,
    always_allow: bool,
) -> Result<(), CommandError> {
    permission_response_with(&state, &id, approved, always_allow).await
}

/// Lists the chat threads, the most recently touched first.
///
/// # Errors
/// As [`chat_list_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn chat_list(state: State<'_, AppState>) -> Result<Vec<ChatRow>, CommandError> {
    chat_list_with(&state)
}

/// Loads one chat thread's messages, oldest first.
///
/// # Errors
/// As [`chat_load_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn chat_load(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<ChatLoadView, CommandError> {
    chat_load_with(&state, &chat_id)
}

/// Renames one chat thread.
///
/// # Errors
/// As [`chat_rename_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn chat_rename(
    state: State<'_, AppState>,
    chat_id: String,
    title: String,
) -> Result<bool, CommandError> {
    chat_rename_with(&state, &chat_id, &title)
}

/// Deletes one chat thread with its messages.
///
/// # Errors
/// As [`chat_delete_with`].
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn chat_delete(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<bool, CommandError> {
    chat_delete_with(&state, &chat_id)
}

/// The host's model config for the sidecar's chat tier, built from the
/// background client's resolved endpoint: the key travels in band on the
/// init envelope, never through the spawn environment.
pub(crate) fn chat_model_config(client: &LlmClient) -> Option<crate::agent::protocol::ModelConfig> {
    let endpoint: crate::LlmEndpoint = client.primary_endpoint(LlmUseCase::AssistantChat)?;
    Some(crate::agent::protocol::ModelConfig {
        api: match endpoint.provider.as_str() {
            "anthropic" => crate::agent::protocol::ModelApi::AnthropicMessages,
            _ => crate::agent::protocol::ModelApi::OpenaiCompletions,
        },
        base_url: endpoint.base_url,
        model: endpoint.model,
        api_key: Some(endpoint.api_key),
    })
}
