// SPDX-License-Identifier: GPL-3.0-only
//! The relay's line routing: one stdout line parsed and dispatched per
//! call, plus the two ledger taps (the sidecar's `llm_call` rows and the
//! audited `tool_result` outcomes).

use astra_ai::{AuditRow, CorrelationId, LlmCallRow, LlmUseCase, ModelTier};

use super::{RelayState, SidecarRelay, StreamPayload};
use crate::agent::permissions::PendingApproval;
use crate::agent::protocol::{
    LlmCallData, LlmCallStatus, OutEnvelope, ToolResultStatus, MAIN_SESSION_ID,
};

impl SidecarRelay {
    /// Parses and routes one stdout line. The read pump enters here with
    /// its generation, and the stale-generation tests replay lines through
    /// the same door.
    ///
    /// Every envelope is gated on that generation, not just the ready
    /// handshake: a line from a pump whose connection was already declared
    /// dead or replaced (a late permission ask, a trailing llm_call)
    /// belongs to a sidecar the host has moved on from, and routing it
    /// would register an ask against the next sidecar or land a ledger row
    /// on the live turn.
    pub(crate) async fn handle_line(&self, line: &str, generation: u64) {
        if self.inner.lock().await.generation != generation {
            log::warn!("agent: dropping a stale sidecar line (generation {generation}): {line}");
            return;
        }
        let envelope: OutEnvelope = match serde_json::from_str(line) {
            Ok(envelope) => envelope,
            Err(e) => {
                log::warn!("agent: skipping an unparseable sidecar line ({e}): {line}");
                return;
            }
        };
        match envelope {
            OutEnvelope::Ready { models, harnesses } => {
                // Emit before the flip: a waiter woken by the Ready state
                // must already observe the ready emission in its sink.
                self.sink.emit_sidecar_ready(models, harnesses);
                self.state_tx.send_replace(RelayState::Ready);
            }
            OutEnvelope::Event { session_id, event } => {
                let correlation = self.correlation_of(&session_id).await;
                self.emit_event(&session_id, correlation, StreamPayload::from(&event));
            }
            OutEnvelope::Done { id } => {
                if let Some(prompt) = self.pending.lock().await.remove(&id) {
                    self.emit_event(
                        &prompt.session_id,
                        Some(prompt.correlation),
                        StreamPayload::Done { prompt_id: id },
                    );
                } else {
                    log::warn!("agent: a done arrived for the unknown prompt {id}");
                }
            }
            OutEnvelope::Error { id, message } => match id {
                Some(id) => {
                    let mut pending = self.pending.lock().await;
                    if let Some(prompt) = pending.remove(&id) {
                        drop(pending);
                        self.emit_event(
                            &prompt.session_id,
                            Some(prompt.correlation),
                            StreamPayload::Error {
                                message,
                                prompt_id: Some(id),
                            },
                        );
                    } else if let Some(sender) = self.commands.lock().await.remove(&id) {
                        let _ = sender.send(Err(message));
                    } else {
                        log::warn!("agent: an error arrived for the unknown id {id}");
                    }
                }
                None => log::warn!("agent: the sidecar reported: {message}"),
            },
            OutEnvelope::Result { id, value } => match self.commands.lock().await.remove(&id) {
                Some(sender) => {
                    let _ = sender.send(Ok(value));
                }
                None => log::warn!("agent: a result arrived for the unknown command {id}"),
            },
            OutEnvelope::LlmCall { call } => self.record_llm_call(call).await,
            OutEnvelope::PermissionRequest {
                id,
                tool_call_id,
                tool,
                level,
                args,
                preview,
            } => {
                let correlation = self.any_correlation().await;
                let approval = PendingApproval {
                    id,
                    tool_call_id,
                    tool,
                    level,
                    args,
                    preview,
                    correlation,
                };
                if let Err(e) = self.bridge.register(approval.clone()) {
                    log::warn!("agent: the bridge refused a permission request: {e}");
                    return;
                }
                self.sink.emit_permission_request(approval);
            }
            OutEnvelope::ToolResult {
                tool_call_id,
                tool,
                status,
                output,
            } => {
                let correlation = self.any_correlation().await;
                self.record_tool_result(&tool, status, output.as_deref(), correlation.as_deref());
                self.emit_event(
                    MAIN_SESSION_ID,
                    correlation,
                    StreamPayload::ToolResult {
                        tool_call_id,
                        name: tool,
                        status: status_word(status).to_string(),
                        output,
                    },
                );
            }
        }
    }

    /// Lands one sidecar-reported LLM call as a ledger row. The envelope's
    /// cache counters have no ledger column and are dropped here; the row
    /// keeps what the ledger records.
    async fn record_llm_call(&self, call: LlmCallData) {
        let live = if let Some(id) = call.correlation_id.clone() {
            CorrelationId::new(id)
        } else {
            let live = self.any_correlation().await;
            if live.is_none() {
                log::warn!(
                    "agent: an llm_call arrived with no correlation id; a fresh one is minted"
                );
            }
            live.map_or_else(CorrelationId::generate, CorrelationId::new)
        };
        let row = LlmCallRow {
            correlation: live,
            use_case: use_case_of(&call.caller),
            tier: tier_of(&call.tier),
            model: call.model,
            prompt_tokens: call.prompt_tokens,
            completion_tokens: call.completion_tokens,
            cost_micros: None,
            latency_ms: call.duration_ms,
            outcome: match call.status {
                LlmCallStatus::Ok => "ok".to_string(),
                LlmCallStatus::Error => match call.error.as_deref() {
                    Some(error) => format!("error: {error}"),
                    None => "error".to_string(),
                },
            },
        };
        if let Err(e) = self.store.record_llm_call(&row) {
            log::warn!("agent: the llm_call ledger row failed: {e}");
        }
    }

    /// Lands the audited outcome of a brokered tool call.
    fn record_tool_result(
        &self,
        tool: &str,
        status: ToolResultStatus,
        output: Option<&str>,
        correlation: Option<&str>,
    ) {
        let correlation = correlation.map_or_else(CorrelationId::generate, |id| {
            CorrelationId::new(id.to_owned())
        });
        let mut arg_preview = output.unwrap_or_default().to_string();
        arg_preview.truncate(200);
        let row = AuditRow {
            correlation,
            verb: tool.to_string(),
            arg_preview,
            outcome: status_word(status).to_string(),
        };
        if let Err(e) = self.store.record_audit(&row) {
            log::warn!("agent: the tool_result audit row failed: {e}");
        }
    }
}

/// The tool-result status word the UI and the audit row carry.
fn status_word(status: ToolResultStatus) -> &'static str {
    match status {
        ToolResultStatus::Ok => "ok",
        ToolResultStatus::Error => "error",
        ToolResultStatus::Rejected => "rejected",
        ToolResultStatus::Skipped => "skipped",
    }
}

/// The ledger use case for a sidecar-reported call: the chat loop's
/// caller word maps to the assistant chat use case, and anything else
/// falls to the same one rather than dropping the row.
fn use_case_of(caller: &str) -> LlmUseCase {
    match caller {
        "agent-chat" => LlmUseCase::AssistantChat,
        other => {
            log::debug!("agent: an unknown llm caller {other} records as assistant chat");
            LlmUseCase::AssistantChat
        }
    }
}

/// The ledger tier for a sidecar-reported call: the five tier words map
/// straight, and the mock harness's own `mock` (or an unconfigured run)
/// records at the chat tier the turn rode.
fn tier_of(tier: &str) -> ModelTier {
    match tier {
        "default" => ModelTier::Default,
        "economy" => ModelTier::Economy,
        "draft" => ModelTier::Draft,
        "nano" => ModelTier::Nano,
        _ => ModelTier::Chat,
    }
}
