// SPDX-License-Identifier: GPL-3.0-only
//! The relay's send paths: the fire-and-forget prompt, the command
//! round trip, the permission answer, the abort, the drains and fail
//! closes every death and restart path settles through, the manual
//! restart, and the deliberate shutdown.

use astra_ai::CorrelationId;

use super::{next_id, PendingPrompt, RelayState, SidecarRelay, StreamPayload};
use crate::agent::protocol::{InCommand, MAIN_SESSION_ID};
use crate::agent::PermissionResolved;

// The test-only methods below name these; the non-test build never does.
#[cfg(test)]
use crate::agent::permissions::PendingApproval;
#[cfg(test)]
use std::sync::Arc;

impl SidecarRelay {
    /// The fire-and-forget prompt: registers the pending turn first (a
    /// sidecar can answer within the same tick), writes the envelope, and
    /// returns the prompt id. The stream arrives through the sink.
    ///
    /// # Errors
    /// Whatever [`SidecarRelay::ensure_started`] or the envelope write
    /// reported; a failed write removes the pending turn again.
    pub(crate) async fn prompt(
        &self,
        text: &str,
        correlation: Option<CorrelationId>,
    ) -> Result<String, String> {
        let arc = self
            .me
            .upgrade()
            .ok_or_else(|| "the agent relay was dropped".to_string())?;
        arc.ensure_started().await?;
        let id = next_id("p");
        let correlation = correlation.unwrap_or_else(CorrelationId::generate);
        self.pending.lock().await.insert(
            id.clone(),
            PendingPrompt {
                session_id: MAIN_SESSION_ID.to_string(),
                correlation: correlation.as_str().to_owned(),
            },
        );
        let command = InCommand::Prompt {
            id: id.clone(),
            session_id: MAIN_SESSION_ID.to_string(),
            text: text.to_owned(),
            correlation_id: correlation.as_str().to_owned(),
        };
        // Capture the generation before the write: if the pump already
        // declared this connection dead, the failure report is stale and
        // the supervisor ignores it, never a double-counted death.
        let generation = self.inner.lock().await.generation;
        if let Err(e) = self.write_command(&command).await {
            self.pending.lock().await.remove(&id);
            arc.on_connection_dead(generation).await;
            return Err(e);
        }
        Ok(id)
    }

    /// Sends one command envelope and resolves its `result` (or `error`).
    /// The tests drive the stub's `ping` through it; the first session
    /// command that needs it arrives with a later slice.
    ///
    /// # Errors
    /// The start or write failure, or the error envelope the sidecar
    /// answered with.
    #[cfg(test)]
    pub(crate) async fn command(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let arc = self
            .me
            .upgrade()
            .ok_or_else(|| "the agent relay was dropped".to_string())?;
        arc.ensure_started().await?;
        let id = next_id("c");
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.commands.lock().await.insert(id.clone(), sender);
        let command = InCommand::Command {
            id: id.clone(),
            name: name.to_owned(),
            args,
        };
        let generation = self.inner.lock().await.generation;
        if let Err(e) = self.write_command(&command).await {
            self.commands.lock().await.remove(&id);
            arc.on_connection_dead(generation).await;
            return Err(e);
        }
        receiver
            .await
            .map_err(|_| format!("the agent command {name} was interrupted"))?
    }

    /// The UI's answer to one pending approval. The bridge consumes the
    /// ask before the response envelope is written; a write failure
    /// restores it so the decision is not lost.
    ///
    /// # Errors
    /// The typed unknown-or-consumed error, or the start and write
    /// failures.
    pub(crate) async fn respond_permission(
        &self,
        id: &str,
        approved: bool,
        always_allow: bool,
    ) -> Result<(), String> {
        let arc = self
            .me
            .upgrade()
            .ok_or_else(|| "the agent relay was dropped".to_string())?;
        arc.ensure_started().await?;
        let consumed = self.bridge.respond(id, approved, always_allow)?;
        let command = InCommand::PermissionResponse {
            id: id.to_owned(),
            approved,
            always_allow,
        };
        let generation = self.inner.lock().await.generation;
        if let Err(e) = self.write_command(&command).await {
            self.bridge.restore(consumed);
            arc.on_connection_dead(generation).await;
            return Err(e);
        }
        self.sink.emit_permission_resolved(PermissionResolved {
            id: id.to_owned(),
            approved,
            always_allow,
        });
        Ok(())
    }

    /// The approvals awaiting a decision, in ask order. The tests read the
    /// bridge through the relay; the UI consumes the ask events instead,
    /// so no runtime reader stands here until a permissions pull command
    /// arrives with a later slice.
    #[cfg(test)]
    pub(crate) fn pending_approvals(&self) -> Vec<PendingApproval> {
        self.bridge.pending()
    }

    /// Cancels one prompt: best effort on the wire, and fail-closes every
    /// pending approval so a suspended tool call cannot outlive the turn
    /// the user cancelled.
    pub(crate) async fn abort(&self, prompt_id: &str) {
        let command = InCommand::Abort {
            id: prompt_id.to_owned(),
            session_id: MAIN_SESSION_ID.to_string(),
        };
        if let Err(e) = self.write_command(&command).await {
            log::debug!("agent: the abort write failed (the sidecar is likely gone): {e}");
        }
        self.fail_close_permissions("prompt aborted");
    }

    /// Settles every pending prompt so the UI never sticks on thinking:
    /// optionally an interrupted state, then always a done.
    pub(crate) async fn drain_pending(&self, interrupted: bool) {
        let drained: Vec<(String, PendingPrompt)> = {
            let mut pending = self.pending.lock().await;
            std::mem::take(&mut *pending).into_iter().collect()
        };
        for (id, prompt) in drained {
            if interrupted {
                self.emit_event(
                    &prompt.session_id,
                    Some(prompt.correlation.clone()),
                    StreamPayload::State {
                        state: "interrupted".to_string(),
                    },
                );
            }
            self.emit_event(
                &prompt.session_id,
                Some(prompt.correlation),
                StreamPayload::Done { prompt_id: id },
            );
        }
    }

    /// Settles every in-flight command with an interrupted error: once the
    /// connection is gone its result will never arrive.
    pub(crate) async fn drain_commands(&self) {
        let drained: Vec<_> = {
            let mut commands = self.commands.lock().await;
            std::mem::take(&mut *commands).into_iter().collect()
        };
        for (id, sender) in drained {
            let _ = sender.send(Err(format!("interrupted: the sidecar died (command {id})")));
        }
    }

    /// Fail-closes every unanswered approval and tells the UI each one
    /// settled: an answer for an ask the (next) sidecar never made must
    /// never be writable.
    pub(crate) fn fail_close_permissions(&self, reason: &str) {
        for approval in self.bridge.fail_close_all(reason) {
            self.sink.emit_permission_resolved(PermissionResolved {
                id: approval.id,
                approved: false,
                always_allow: false,
            });
        }
    }

    /// Kills the sidecar, resets the restart budget, and runs a fresh
    /// handshake. The tests drive it directly; a user-facing reconnect
    /// gesture arrives with a later slice and lifts the gate.
    ///
    /// # Errors
    /// Whatever the fresh [`SidecarRelay::ensure_started`] reports.
    #[cfg(test)]
    pub(crate) async fn restart(self: &Arc<Self>) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().await;
            if let Some(mut killer) = inner.killer.take() {
                killer.start_kill();
            }
            inner.writer = None;
            inner.attempts = 0;
            // Invalidate the about-to-die pump so its EOF does not schedule
            // a supervisor restart beside this manual one.
            inner.generation += 1;
        }
        self.drain_pending(true).await;
        self.drain_commands().await;
        self.fail_close_permissions("sidecar restart");
        self.state_tx.send_replace(RelayState::Down);
        self.ensure_started().await
    }

    /// Kills the sidecar and settles everything with a plain done (a
    /// deliberate shutdown, not a failure).
    pub(crate) async fn shutdown(&self) {
        {
            let mut inner = self.inner.lock().await;
            if let Some(mut killer) = inner.killer.take() {
                killer.start_kill();
            }
            inner.writer = None;
            inner.generation += 1;
        }
        self.drain_pending(false).await;
        self.drain_commands().await;
        self.fail_close_permissions("app shutdown");
        self.state_tx.send_replace(RelayState::Down);
    }

    /// Plants an approval straight into the bridge (the supervisor tests'
    /// way to hold an ask open across an induced death).
    #[cfg(test)]
    pub(crate) fn register_test_approval(&self, approval: PendingApproval) {
        self.bridge
            .register(approval)
            .expect("the test ask registers");
    }
}
