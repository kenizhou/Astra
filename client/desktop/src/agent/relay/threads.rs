// SPDX-License-Identifier: GPL-3.0-only
//! The relay's chat-thread fold: the per-turn accumulator that mirrors the
//! rendered log as the events cross, and the store write that lands the
//! assistant's row. The fold point is the turn's settle: the row records
//! in the relay's `done`, `error`, and drain routing BEFORE the settle
//! event crosses to the UI, so a panel that reloads on the settle reads
//! the row the live log just became. The user's side records at dispatch
//! ([`SidecarRelay::prompt`]): the reference's own order.

use std::collections::HashMap;

use astra_ai::{ChatId, ChatRole};

use super::SidecarRelay;
use crate::agent::StreamPayload;

/// The assistant chat's pipeline version, the AI rules' bump pin: every
/// behaviour change to the chat pipeline (prompts, the tool surface, the
/// fold the rows record) bumps this and lands its eval in the same
/// change. Every run stamps it into its chat row's metadata.
pub(crate) const AGENT_PIPELINE_VERSION: &str = "1";

/// How a turn settled, as the run metadata's finish reason records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnFinish {
    /// The done envelope arrived.
    Done,
    /// An error envelope answered the prompt.
    Error,
    /// A death or shutdown drain settled the turn.
    Interrupted,
}

impl TurnFinish {
    /// The metadata word, with the turn's own abort flag applied.
    fn word(self, aborted: bool) -> &'static str {
        match (self, aborted) {
            (Self::Done, true) => "aborted",
            (Self::Done, false) => "done",
            (Self::Error, _) => "error",
            (Self::Interrupted, _) => "interrupted",
        }
    }
}

/// One live turn's accumulator: the chat the turn records into, the
/// rendered parts in arrival order, the tool calls awaiting their result
/// envelopes, and the run facts the metadata row carries. Incomplete tool
/// calls are dropped at the close (the reference's rule: an aborted turn
/// records what rendered, never a half call).
struct LiveTurn {
    chat: ChatId,
    correlation: String,
    session: String,
    parts: Vec<serde_json::Value>,
    staged: HashMap<String, (String, serde_json::Value)>,
    model: Option<(String, String)>,
    tool_calls: u64,
    aborted: bool,
}

/// The live turns by prompt id, beside the tool-call ownership map that
/// routes the result envelopes the sidecar sends without a session of
/// their own. Plain sync state: never held across an await.
#[derive(Default)]
pub(crate) struct TurnBook {
    turns: HashMap<String, LiveTurn>,
    tool_owners: HashMap<String, String>,
}

impl SidecarRelay {
    /// Opens one turn's accumulator. Turns without a chat (the main
    /// session's prompts) never open one and so never record.
    pub(crate) fn turn_open(
        &self,
        prompt_id: &str,
        chat: ChatId,
        correlation: &str,
        session: &str,
    ) {
        self.turns
            .lock()
            .expect("the turn book is not poisoned")
            .turns
            .insert(
                prompt_id.to_owned(),
                LiveTurn {
                    chat,
                    correlation: correlation.to_owned(),
                    session: session.to_owned(),
                    parts: Vec::new(),
                    staged: HashMap::new(),
                    model: None,
                    tool_calls: 0,
                    aborted: false,
                },
            );
    }

    /// Folds one streamed event into the turn's rendered parts: text
    /// merges into the trailing text run, a tool call stages for its
    /// result (and claims its call id, which routes the result envelope
    /// that names no session of its own), and the budget's word lands as
    /// a note at its position.
    pub(crate) fn turn_fold_event(&self, prompt_id: &str, event: &StreamPayload) {
        let mut book = self.turns.lock().expect("the turn book is not poisoned");
        let TurnBook { turns, tool_owners } = &mut *book;
        let Some(turn) = turns.get_mut(prompt_id) else {
            return;
        };
        match event {
            StreamPayload::TextDelta { text } => {
                if let Some(last) = turn.parts.last_mut() {
                    if last["kind"] == "text" {
                        let merged =
                            format!("{}{}", last["text"].as_str().unwrap_or_default(), text);
                        last["text"] = serde_json::Value::String(merged);
                        return;
                    }
                }
                turn.parts
                    .push(serde_json::json!({ "kind": "text", "text": text }));
            }
            StreamPayload::ToolCall {
                name,
                args,
                tool_call_id,
            } => {
                turn.staged
                    .insert(tool_call_id.clone(), (name.clone(), args.clone()));
                tool_owners.insert(tool_call_id.clone(), prompt_id.to_owned());
            }
            StreamPayload::State { state } => {
                if state == "budget-spent" {
                    turn.parts.push(serde_json::json!({
                        "kind": "note",
                        "text": "the tool budget is spent; the assistant finishes in text",
                    }));
                }
            }
            StreamPayload::ToolResult { .. }
            | StreamPayload::Done { .. }
            | StreamPayload::Error { .. } => {}
        }
    }

    /// Completes one staged tool call with its audited outcome and answers
    /// the session and correlation the result event should carry: the
    /// result envelopes name no session of their own, so ownership of the
    /// call routes them. A result nothing staged answers `None` (a foreign
    /// or late call; the caller falls back to the main session).
    pub(crate) fn turn_complete_tool(
        &self,
        tool_call_id: &str,
        name: &str,
        status: &str,
        output: Option<&str>,
    ) -> Option<(String, String)> {
        let mut book = self.turns.lock().expect("the turn book is not poisoned");
        let prompt_id = book.tool_owners.remove(tool_call_id)?;
        let turn = book.turns.get_mut(&prompt_id)?;
        let staged = turn.staged.remove(tool_call_id);
        turn.tool_calls += 1;
        let mut part = serde_json::json!({
            "kind": "tool",
            "name": name,
            "status": status,
        });
        if let Some((_, args)) = staged {
            part["args"] = args;
        }
        if let Some(output) = output {
            part["output"] = serde_json::Value::String(output.to_owned());
        }
        turn.parts.push(part);
        Some((turn.session.clone(), turn.correlation.clone()))
    }

    /// The turn that staged a tool call, without completing it: the ask
    /// that arrives while the call waits routes through the same ownership
    /// the result envelope does. A call nothing staged answers `None`.
    pub(crate) fn turn_owning_call(&self, tool_call_id: &str) -> Option<(String, String)> {
        let book = self.turns.lock().expect("the turn book is not poisoned");
        let prompt_id = book.tool_owners.get(tool_call_id)?;
        let turn = book.turns.get(prompt_id)?;
        Some((turn.session.clone(), turn.correlation.clone()))
    }

    /// Notes the run's model and tier from the turn's llm_call rows: the
    /// last call's pair is what the metadata records.
    pub(crate) fn turn_note_llm(&self, correlation: Option<&str>, model: &str, tier: &str) {
        let mut book = self.turns.lock().expect("the turn book is not poisoned");
        let turn = match correlation {
            Some(correlation) => book
                .turns
                .values_mut()
                .find(|turn| turn.correlation == correlation),
            None => book.turns.values_mut().next(),
        };
        if let Some(turn) = turn {
            turn.model = Some((model.to_owned(), tier.to_owned()));
        }
    }

    /// Marks a turn the host aborted, so its done records the abort as the
    /// finish reason.
    pub(crate) fn turn_mark_aborted(&self, prompt_id: &str) {
        if let Some(turn) = self
            .turns
            .lock()
            .expect("the turn book is not poisoned")
            .turns
            .get_mut(prompt_id)
        {
            turn.aborted = true;
        }
    }

    /// Closes one turn at its settle: what rendered folds into the chat's
    /// assistant row, the run metadata beside it. A turn that rendered
    /// nothing (a refused prompt, a death before the first event) records
    /// no row; the user's side of that turn already stands.
    pub(crate) fn turn_close(&self, prompt_id: &str, finish: TurnFinish) {
        let turn = self
            .turns
            .lock()
            .expect("the turn book is not poisoned")
            .turns
            .remove(prompt_id);
        let Some(turn) = turn else {
            return;
        };
        for owner in turn.staged.keys() {
            self.turns
                .lock()
                .expect("the turn book is not poisoned")
                .tool_owners
                .remove(owner);
        }
        if turn.parts.is_empty() {
            return;
        }
        let mut metadata = serde_json::json!({
            "pipeline": AGENT_PIPELINE_VERSION,
            "finish_reason": finish.word(turn.aborted),
            "tool_calls": turn.tool_calls,
            "correlation": turn.correlation,
        });
        if let Some((model, tier)) = turn.model {
            metadata["model"] = serde_json::Value::String(model);
            metadata["tier"] = serde_json::Value::String(tier);
        }
        let parts = serde_json::Value::Array(turn.parts);
        if let Err(e) =
            self.store
                .insert_chat_message(&turn.chat, ChatRole::Assistant, &parts, Some(&metadata))
        {
            log::warn!("agent: the chat's assistant row failed: {e}");
        }
    }
}
