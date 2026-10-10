// SPDX-License-Identifier: GPL-3.0-only
//! The Rust mirror of the sidecar's JSONL protocol: one JSON object per
//! line in both directions, stderr carrying human logs only. The wire is
//! internally tagged (`"type"` on envelopes) with snake_case fields, pinned
//! by the shared fixtures under `sidecar/src/fixtures/protocol`, which the
//! tests below parse and re-serialise through this mirror so the Rust host
//! and the TypeScript sidecar cannot drift.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One wire protocol a tier config may name, spelled kebab-case on the
/// wire (the pi SDK's adapter names).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ModelApi {
    /// The OpenAI chat-completions wire shape.
    #[default]
    #[serde(rename = "openai-completions")]
    OpenaiCompletions,
    /// The Anthropic messages wire shape.
    #[serde(rename = "anthropic-messages")]
    AnthropicMessages,
}

/// One tier's endpoint config as the init envelope delivers it. The key
/// rides in band, never in the spawn environment, and lives only in sidecar
/// memory once there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModelConfig {
    /// The wire protocol the endpoint speaks; always emitted (the sidecar's
    /// own defaulting only forgives its absence on parse, and this mirror
    /// matches it with a default of its own).
    #[serde(default)]
    pub api: ModelApi,
    /// The endpoint base URL.
    pub base_url: String,
    /// The provider's model name.
    pub model: String,
    /// The provider key; absent when the tier runs unauthenticated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// One policy row's gate level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PermissionLevel {
    /// Run without asking.
    Auto,
    /// Run and tell the user afterwards.
    Notify,
    /// Ask before running.
    Confirm,
    /// Ask before running, showing a preview.
    ConfirmPreview,
}

/// One row of the permission policy table the host pushes on init.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PermissionPolicy {
    /// The gate the tool runs under.
    pub level: PermissionLevel,
    /// Whether the tool's own allowlist, when it passes, short-circuits the
    /// gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowlist: Option<bool>,
}

/// The host-to-sidecar commands, one per stdin line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum InCommand {
    /// The init handshake: everything the sidecar runs on, answered by
    /// `ready`.
    Init {
        /// The harness to run: `pi` or `mock`.
        harness: String,
        /// The absolute astra-cli path; the empty string means unresolved
        /// and safe-exec fails loudly at call time.
        cli_path: String,
        /// The app data directory the sidecar and its tool children use.
        data_dir: String,
        /// The mock transcript fixture; the mock harness only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mock_fixture: Option<String>,
        /// Tier name to endpoint config; omitted when no tier is
        /// configured.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_configs: Option<BTreeMap<String, ModelConfig>>,
        /// The permission policy table; absent means every tool fails
        /// closed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        policy: Option<BTreeMap<String, PermissionPolicy>>,
    },
    /// One chat turn. The correlation id rides every tool spawn and the
    /// llm_call ledger row of the turn.
    Prompt {
        /// The host-assigned prompt id.
        id: String,
        /// The session the turn belongs to.
        session_id: String,
        /// The user's text.
        text: String,
        /// The turn's correlation id.
        correlation_id: String,
    },
    /// Cancel one prompt.
    Abort {
        /// The prompt to cancel.
        id: String,
        /// The session it belongs to.
        session_id: String,
    },
    /// One sidecar command, answered by `result` or `error`.
    Command {
        /// The host-assigned command id.
        id: String,
        /// The command name.
        name: String,
        /// The command arguments.
        args: serde_json::Value,
    },
    /// The UI's answer to a permission request.
    PermissionResponse {
        /// The request being answered.
        id: String,
        /// Whether the user approved.
        approved: bool,
        /// Whether to remember the approval for the tool.
        always_allow: bool,
    },
}

/// One streamed event payload inside an `event` envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum AgentEvent {
    /// A chunk of assistant text.
    TextDelta {
        /// The text chunk.
        text: String,
    },
    /// The model invoked a tool.
    ToolCall {
        /// The tool's name.
        name: String,
        /// The tool's arguments.
        args: serde_json::Value,
        /// The call's id, for pairing with its result.
        tool_call_id: String,
    },
    /// A harness state word, for example `thinking`.
    State {
        /// The state word.
        state: String,
    },
    /// A tool call finished.
    ToolResult {
        /// The finished call's id.
        tool_call_id: String,
        /// The tool's name.
        name: String,
        /// How the call ended.
        status: String,
        /// The tool's output, when it produced one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
}

/// How a brokered tool call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolResultStatus {
    /// The call ran and succeeded.
    Ok,
    /// The call ran and failed.
    Error,
    /// A permission refusal stopped it.
    Rejected,
    /// It was cancelled before running.
    Skipped,
}

/// How one LLM API call ended, as the ledger row records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LlmCallStatus {
    /// The call returned a completion.
    Ok,
    /// The call failed.
    Error,
}

/// One LLM API call's ledger row, emitted per call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LlmCallData {
    /// The session the call served.
    pub session_id: String,
    /// The prompt the call served.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    /// The turn's correlation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// The model that answered.
    pub model: String,
    /// The tier the call ran at.
    pub tier: String,
    /// Why the call was made.
    pub caller: String,
    /// Tokens reported for the prompt.
    pub prompt_tokens: u64,
    /// Tokens reported for the completion.
    pub completion_tokens: u64,
    /// Cache-read tokens reported.
    pub cache_read_tokens: u64,
    /// Cache-write tokens reported.
    pub cache_write_tokens: u64,
    /// The call's wall time in milliseconds.
    pub duration_ms: u64,
    /// How the call ended.
    pub status: LlmCallStatus,
    /// The failure, when the call failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The sidecar-to-host envelopes, one per stdout line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum OutEnvelope {
    /// The init handshake's answer.
    Ready {
        /// The models the sidecar can run.
        models: Vec<String>,
        /// The harnesses it knows.
        harnesses: Vec<String>,
    },
    /// One streamed event of a session.
    Event {
        /// The session the event belongs to.
        session_id: String,
        /// The event.
        event: AgentEvent,
    },
    /// A prompt settled.
    Done {
        /// The prompt that settled.
        id: String,
    },
    /// An error, for a prompt or command id, or the sidecar at large.
    Error {
        /// The id the error answers; null when none.
        id: Option<String>,
        /// The failure.
        message: String,
    },
    /// A command's answer.
    Result {
        /// The command being answered.
        id: String,
        /// The answer.
        value: serde_json::Value,
    },
    /// One LLM API call's ledger row.
    LlmCall {
        /// The row.
        call: LlmCallData,
    },
    /// A tool call needs the user's decision.
    PermissionRequest {
        /// The request id the answer names.
        id: String,
        /// The tool call waiting on the decision.
        tool_call_id: String,
        /// The tool's name.
        tool: String,
        /// The gate level the tool ran under.
        level: PermissionLevel,
        /// The tool's arguments.
        args: serde_json::Value,
        /// Renderable preview payload, when the tool produced one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview: Option<serde_json::Value>,
    },
    /// The audited outcome of a brokered tool call.
    ToolResult {
        /// The call's id.
        tool_call_id: String,
        /// The tool's name.
        tool: String,
        /// How the call ended.
        status: ToolResultStatus,
        /// The tool's output, when it produced one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
}

/// The single global chat session id (the fixtures' `agent:main`): SPA0
/// chat is one session, and every prompt of it carries its own correlation
/// id.
pub(crate) const MAIN_SESSION_ID: &str = "agent:main";

#[cfg(test)]
mod tests {
    use super::*;

    /// One pinned fixture: its name and its bytes, embedded at compile time
    /// from the sidecar's shared fixture directory (the same files its own
    /// protocol test parses, so neither side can drift).
    macro_rules! fixture {
        ($name:literal) => {
            (
                $name,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../sidecar/src/fixtures/protocol/",
                    $name
                )),
            )
        };
    }

    /// Every pinned fixture, both directions.
    const FIXTURES: &[(&str, &str)] = &[
        fixture!("in-init.json"),
        fixture!("in-init-mock.json"),
        fixture!("in-prompt.json"),
        fixture!("in-abort.json"),
        fixture!("in-command.json"),
        fixture!("in-permission-response.json"),
        fixture!("out-ready.json"),
        fixture!("out-event-state.json"),
        fixture!("out-event-text-delta.json"),
        fixture!("out-event-tool-call.json"),
        fixture!("out-event-tool-result.json"),
        fixture!("out-done.json"),
        fixture!("out-error.json"),
        fixture!("out-result.json"),
        fixture!("out-llm-call.json"),
        fixture!("out-permission-request.json"),
        fixture!("out-tool-result.json"),
    ];

    /// One fixture parses, its serialisation is stable (serialise, parse,
    /// serialise again: the same bytes both times), and the serialisation
    /// is semantically the fixture re-parsed. For every fixture whose maps
    /// (the policy table, the model configs) arrive in one order this is
    /// byte for byte the fixture too: the mirror's maps are `BTreeMap`s,
    /// so their keys serialise sorted, and JSON object order carries no
    /// meaning on this wire (the sidecar's parser is order-blind; its own
    /// round trip preserves file order only because JS objects keep
    /// insertion order).
    #[test]
    fn every_fixture_round_trips_faithfully() {
        for (name, raw) in FIXTURES {
            let line = raw.trim_end();
            let (back, again) = if name.starts_with("in-") {
                let cmd: InCommand = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("fixture {name} parses: {e}"));
                let once = serde_json::to_string(&cmd).expect("serialises");
                let twice = serde_json::to_string(
                    &serde_json::from_str::<InCommand>(&once).expect("the mirror's form reparses"),
                )
                .expect("serialises");
                (once, twice)
            } else {
                let env: OutEnvelope = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("fixture {name} parses: {e}"));
                let once = serde_json::to_string(&env).expect("serialises");
                let twice = serde_json::to_string(
                    &serde_json::from_str::<OutEnvelope>(&once)
                        .expect("the mirror's form reparses"),
                )
                .expect("serialises");
                (once, twice)
            };
            assert_eq!(back, again, "fixture {name}'s serialisation is stable");
            if back == line {
                continue;
            }
            // Not byte identical: only the map key order may differ. Compare
            // as values, so any other drift still fails.
            let parsed_again = if name.starts_with("in-") {
                serde_json::to_value(
                    serde_json::from_str::<InCommand>(line).expect("the fixture parses"),
                )
                .expect("values")
            } else {
                serde_json::to_value(
                    serde_json::from_str::<OutEnvelope>(line).expect("the fixture parses"),
                )
                .expect("values")
            };
            let parsed_back = serde_json::to_value(
                serde_json::from_str::<serde_json::Value>(&back).expect("the back form parses"),
            )
            .expect("values");
            assert_eq!(
                parsed_again, parsed_back,
                "fixture {name}'s round trip differs beyond map key order"
            );
        }
    }

    #[test]
    fn an_absent_model_api_defaults_to_openai_completions() {
        let env: OutEnvelope =
            serde_json::from_str(r#"{"type":"ready","models":[],"harnesses":[]}"#).expect("parses");
        let OutEnvelope::Ready { .. } = env else {
            panic!("ready variant");
        };
        let cmd: InCommand = serde_json::from_str(
            r#"{"type":"init","harness":"pi","cli_path":"","data_dir":"/d",
                "model_configs":{"chat":{"base_url":"https://e","model":"m"}}}"#,
        )
        .expect("parses");
        let InCommand::Init { model_configs, .. } = cmd else {
            panic!("init variant");
        };
        let config = model_configs.expect("configs present").get("chat").cloned();
        assert_eq!(
            config.map(|c| c.api),
            Some(ModelApi::OpenaiCompletions),
            "the sidecar's defaulting is mirrored"
        );
    }
}
