// SPDX-License-Identifier: GPL-3.0-only
//! The agent command surface: thin Tauri shells over the relay, following
//! the host's inner-function convention so the tests drive the same seam
//! the webview invokes. [`agent_prompt_with`] generates the turn's
//! correlation id when the caller brought none, and the id echoes on every
//! event, ledger row, and audit row of the turn.

use astra_ai::{CorrelationId, LlmUseCase};
use tauri::State;

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

/// Sends one prompt and returns its id. The correlation id is the
/// caller's when it brought one and a fresh uuid otherwise; either way it
/// rides the prompt envelope and echoes on every event of the turn.
///
/// # Errors
/// Returns [`CommandError::Agent`] when the sidecar cannot be started or
/// the prompt envelope cannot be written.
pub(crate) async fn agent_prompt_with(
    state: &AppState,
    prompt: &str,
    correlation: Option<String>,
) -> Result<String, CommandError> {
    let correlation = correlation.map(CorrelationId::new);
    state
        .agent
        .prompt(prompt, correlation)
        .await
        .map_err(CommandError::Agent)
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

/// Sends one prompt to the agent, returning the prompt id its events
/// carry.
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
) -> Result<String, CommandError> {
    agent_prompt_with(&state, &prompt, correlation).await
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
