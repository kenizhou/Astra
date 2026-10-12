// SPDX-License-Identifier: GPL-3.0-only
//! Deterministic execution: the closed action set applied to a matched
//! message through the same write paths the list intents drive
//! (mark-read and flag over the engine's outbox), each execution landing
//! an audit row under the pass's correlation id. No model runs here and
//! no outcome is guessed: an action either goes through the deterministic
//! path or it does not run at all.

use astra_ai::{AiStore, AuditRow, CorrelationId};
use engine_api::Provider;

use super::Action;
use crate::reference::{MessageRef, RowRef};
use crate::App;

impl<P: Provider> App<P> {
    /// Applies `actions` to `message` through the existing write paths:
    /// the very methods [`Intent::MarkRead`](crate::Intent::MarkRead) and
    /// [`Intent::SetFlagged`](crate::Intent::SetFlagged) dispatch to, outbox
    /// and republish included, so an AI-driven write and a user-driven one
    /// are one behaviour. Each action lands one audit row under the pass's
    /// `correlation`; the preview carries the action and the message's
    /// identity halves and never mail content, and the outcome word `ok`
    /// names the dispatch, the same discard-and-resync honesty the write
    /// paths themselves carry (the refreshed snapshot publishes what
    /// actually holds).
    pub(crate) async fn execute_actions(
        &self,
        message: &MessageRef,
        actions: &[Action],
        correlation: &CorrelationId,
        audit: &AiStore,
    ) {
        for action in actions {
            // The outcome word names the dispatch: `ok` when the write went
            // through its path, `error` when the account had vanished before
            // it could. What the write's own discard-and-resync discipline
            // carries (the server's eventual answer) stays in the snapshot,
            // not here.
            let dispatched = match action {
                Action::MarkRead => {
                    match self.mark_read(RowRef::Message(message.clone()), true).await {
                        Ok(()) => true,
                        Err(err) => {
                            log::warn!("rules execute: mark-read: {err}");
                            false
                        }
                    }
                }
                Action::SetFlagged => {
                    match self
                        .set_flagged(RowRef::Message(message.clone()), true)
                        .await
                    {
                        Ok(()) => true,
                        Err(err) => {
                            log::warn!("rules execute: flag: {err}");
                            false
                        }
                    }
                }
            };
            let row = AuditRow {
                correlation: correlation.clone(),
                verb: "ai_rules execute".to_owned(),
                arg_preview: format!(
                    "{} {} {}",
                    action,
                    message.account.as_str(),
                    message.key.as_str(),
                ),
                outcome: if dispatched { "ok" } else { "error" }.to_owned(),
            };
            if let Err(err) = audit.record_audit(&row) {
                log::warn!("ai rules: the execution audit could not be recorded: {err}");
            }
        }
    }
}
