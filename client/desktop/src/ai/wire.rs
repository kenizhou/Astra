// SPDX-License-Identifier: GPL-3.0-only
//! The dual-protocol wire: request construction, response parsing,
//! non-2xx rejection classification, and the balanced-brace JSON repair.
//! The endpoints, header sets, and body shapes mirror kylins' shared LLM
//! core (`kylins.client.backend/src/agent/
//! llm_client.rs` with its `build_chat_request` and `build_body`): openai
//! entries post to `{base}/chat/completions` with a bearer token, anthropic
//! entries post to `{base}/v1/messages` with the `x-api-key` and
//! `anthropic-version` headers and the mandatory `max_tokens`.

use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::Serialize;
use serde_json::Value;

/// The wire protocol a tier entry's provider prefix selects: the two
/// protocols kylins' `MODEL_APIS` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Protocol {
    /// openai-completions: `POST {base}/chat/completions`, `Authorization:
    /// Bearer`.
    OpenAiCompletions,
    /// anthropic-messages: `POST {base}/v1/messages`, `x-api-key` plus
    /// `anthropic-version`.
    AnthropicMessages,
}

/// Maps a tier entry's provider prefix to its protocol. Any other provider is
/// unknown to this build and the entry is skipped at call time (kylins'
/// unknown-protocol screening).
pub(crate) fn protocol_for(provider: &str) -> Option<Protocol> {
    match provider {
        "openai" => Some(Protocol::OpenAiCompletions),
        "anthropic" => Some(Protocol::AnthropicMessages),
        _ => None,
    }
}

impl Protocol {
    /// The provider's default endpoint base: the SDK defaults kylins'
    /// endpoint doc cites. openai serves chat completions under `/v1`;
    /// anthropic's base is the bare host because the version prefix belongs
    /// to the path.
    pub(crate) fn default_base(self) -> &'static str {
        match self {
            Self::OpenAiCompletions => "https://api.openai.com/v1",
            Self::AnthropicMessages => "https://api.anthropic.com",
        }
    }
}

/// The completion cap every anthropic-messages request carries: that
/// protocol makes `max_tokens` mandatory, and kylins' callers pass their own
/// cap on the same field.
pub(crate) const ANTHROPIC_MAX_TOKENS: u32 = 1024;

/// The ceiling on a provider's `Retry-After` word. The wait before the next
/// chain entry is honoured but bounded, so a hostile or mistaken header
/// cannot stall a background call.
pub(crate) const RETRY_AFTER_CAP: Duration = Duration::from_secs(2);

/// How a non-2xx response rejects the request: the inbox-zero error classes
/// (`shouldFallbackToNextModel` and the insufficient-credit classifiers)
/// ported onto raw HTTP statuses and bodies.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rejection {
    /// Out of credit or quota (HTTP 402, OpenAI's `insufficient_quota` 429,
    /// or Anthropic's `insufficient_balance` message). Terminal for the
    /// call.
    Quota,
    /// A rate limit: advance the chain, honouring `Retry-After`.
    RateLimited,
    /// Any other non-2xx: advance the chain without a wait. A model-specific
    /// 400 or 404 can pass on the next entry, the inbox-zero invalid-model
    /// fallback.
    Http,
}

/// Classifies one non-2xx response. Auth (401 or 403) is classified earlier,
/// before the body is consulted.
pub(crate) fn classify_rejection(status: u16, body: &str) -> Rejection {
    // Exhausted balances arrive as retryable-looking statuses, so the body is
    // looked at beneath the status (the inbox-zero exhausted-balance rule).
    if status == 402
        || body.contains("insufficient_quota")
        || body.contains("insufficient_balance")
        || body.contains("credit balance is too low")
    {
        return Rejection::Quota;
    }
    if status == 429 {
        return Rejection::RateLimited;
    }
    Rejection::Http
}

/// The `Retry-After` word as the wait before the next chain entry, capped.
/// Only delta-seconds parse; an HTTP-date or garbage reads as absent.
pub(crate) fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds.min(RETRY_AFTER_CAP.as_secs())))
}

/// One chat message: the user-message shape both protocols share.
#[derive(Debug, Serialize)]
pub(crate) struct ChatMessage<'a> {
    /// The role; a completion sends only `user`.
    role: &'a str,
    /// The message text.
    content: &'a str,
}

/// The openai-completions request body: the model and the message list
/// (kylins' `build_body` openai arm). `response_format` stays out because
/// `complete` serves prose and structured outputs alike; the structured
/// path recovers through the AI foundation's shared `astra_ai::repair_json`
/// instead.
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiBody<'a> {
    /// The provider's model name.
    model: &'a str,
    /// The message list.
    messages: Vec<ChatMessage<'a>>,
}

/// The anthropic-messages request body: the model, the mandatory
/// `max_tokens`, and the message list (kylins' `build_body` anthropic arm).
#[derive(Debug, Serialize)]
pub(crate) struct AnthropicBody<'a> {
    /// The provider's model name.
    model: &'a str,
    /// The completion cap the protocol mandates.
    max_tokens: u32,
    /// The message list.
    messages: Vec<ChatMessage<'a>>,
}

/// One fully built wire request: the exact URL, the protocol's headers, and
/// the serialised body (sent as pre-serialised bytes so the tests pin them).
/// Deliberately not `Debug`: the headers carry the provider key.
pub(crate) struct WireRequest {
    /// The exact URL the request targets.
    pub(crate) url: String,
    /// The protocol's headers, order-stable.
    pub(crate) headers: Vec<(&'static str, String)>,
    /// The serialised JSON body.
    pub(crate) body: String,
}

/// Builds the dual-protocol request (kylins' `build_chat_request` merged with
/// its `build_body`, over a caller-owned HTTP client). The body is
/// serialised here, once, so what goes on the wire is exactly what the tests
/// pin.
pub(crate) fn build_request(
    protocol: Protocol,
    base_url: &str,
    model: &str,
    prompt: &str,
    api_key: &str,
) -> WireRequest {
    let user = ChatMessage {
        role: "user",
        content: prompt,
    };
    match protocol {
        Protocol::OpenAiCompletions => {
            let base = base_url.trim_end_matches('/');
            WireRequest {
                url: format!("{base}/chat/completions"),
                headers: vec![
                    ("authorization", format!("Bearer {api_key}")),
                    ("content-type", "application/json".to_string()),
                ],
                body: serde_json::to_string(&OpenAiBody {
                    model,
                    messages: vec![user],
                })
                .expect("a struct of string fields serialises"),
            }
        }
        Protocol::AnthropicMessages => {
            // Forgive a pasted `/v1` suffix (with or without trailing slash):
            // the version prefix is appended exactly once.
            let mut base = base_url.trim_end_matches('/');
            if let Some(stripped) = base.strip_suffix("/v1") {
                base = stripped.trim_end_matches('/');
            }
            WireRequest {
                url: format!("{base}/v1/messages"),
                headers: vec![
                    ("x-api-key", api_key.to_string()),
                    ("anthropic-version", "2023-06-01".to_string()),
                    ("content-type", "application/json".to_string()),
                ],
                body: serde_json::to_string(&AnthropicBody {
                    model,
                    max_tokens: ANTHROPIC_MAX_TOKENS,
                    messages: vec![user],
                })
                .expect("a struct of string fields serialises"),
            }
        }
    }
}

/// What one 2xx envelope carried: the model's text plus the usage the ledger
/// records.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Completion {
    /// The model's text content.
    pub(crate) content: String,
    /// The token counters and cost the envelope reported.
    pub(crate) usage: Usage,
}

/// The usage block, normalised across the two protocols. Absent counters
/// read as zero and an absent cost as `None`: the ledger stays truthful
/// about what the provider said.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    /// Tokens reported for the prompt (anthropic `input_tokens`).
    pub(crate) prompt_tokens: u64,
    /// Tokens reported for the completion (anthropic `output_tokens`).
    pub(crate) completion_tokens: u64,
    /// The provider-reported cost in micros when the envelope carries one
    /// (the OpenRouter `usage.cost` convention).
    pub(crate) cost_micros: Option<u64>,
}

/// Extracts the completion and usage from a 2xx envelope, per protocol
/// (kylins' `extract_content` and `extract_usage`). A 2xx that is not JSON or
/// carries no text content is an error the caller classifies retryable:
/// kylins retries malformed envelopes rather than degrading to them.
pub(crate) fn parse_response(protocol: Protocol, body: &str) -> Result<Completion, String> {
    let envelope: Value =
        serde_json::from_str(body).map_err(|e| format!("2xx envelope is not JSON: {e}"))?;
    let content = match protocol {
        Protocol::AnthropicMessages => {
            let mut found = None;
            if let Some(blocks) = envelope.get("content").and_then(Value::as_array) {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            found = Some(text.to_string());
                            break;
                        }
                    }
                }
            }
            found
        }
        Protocol::OpenAiCompletions => envelope
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    let Some(content) = content else {
        return Err("2xx envelope has no text content".to_string());
    };
    let usage = envelope.get("usage");
    let counter = |key: &str| {
        usage
            .and_then(|block| block.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let tokens = match protocol {
        Protocol::AnthropicMessages => (counter("input_tokens"), counter("output_tokens")),
        Protocol::OpenAiCompletions => (counter("prompt_tokens"), counter("completion_tokens")),
    };
    Ok(Completion {
        content,
        usage: Usage {
            prompt_tokens: tokens.0,
            completion_tokens: tokens.1,
            cost_micros: cost_to_micros(
                usage
                    .and_then(|block| block.get("cost"))
                    .and_then(Value::as_f64),
            ),
        },
    })
}

/// Converts a provider-reported fractional cost to micros. A negative,
/// infinite, or absurdly large cost folds to `None`: the ledger records only
/// what it can state truthfully. The bound keeps the product inside the
/// range where an f64 still names its integer exactly.
fn cost_to_micros(cost: Option<f64>) -> Option<u64> {
    let cost = cost?;
    if !(0.0..9.0e9).contains(&cost) {
        return None;
    }
    format!("{:.0}", cost * 1_000_000.0).parse().ok()
}
