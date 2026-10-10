// SPDX-License-Identifier: GPL-3.0-only
//! The background LLM client: one completion call resolves the use case's
//! tier chain, sends each entry's dual-protocol wire request, and lands one
//! ledger row per HTTP attempt. Retryable failures advance the fallback
//! chain; auth and quota rejections are terminal for the call (the
//! inbox-zero fallback-on classes over raw HTTP, the kylins row contract).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use astra_ai::{AiStore, CorrelationId, LlmCallRow, LlmUseCase, ModelEntry, TierTable};
use log::warn;
use reqwest::{
    header::{HeaderMap, RETRY_AFTER},
    Client, StatusCode,
};

use crate::ai::wire::{self, Completion, WireRequest};

/// The per-attempt timeout: real completions are slower than a one-token
/// probe, so the client waits as long as kylins' shared core does.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The ceiling on a provider's `Retry-After` word. The wait before the next
/// chain entry is honoured but bounded, so a hostile or mistaken header
/// cannot stall a background call.
const RETRY_AFTER_CAP: Duration = Duration::from_secs(2);

/// The typed failure of one completion call.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// No chain stands behind the use case, or no entry of the chain has
    /// both a known protocol and a key. Nothing was attempted, so nothing
    /// was recorded.
    #[error("llm not configured: {0}")]
    NotConfigured(String),
    /// The provider rejected the credentials (401 or 403): a retry cannot
    /// fix a bad key, so the call is terminal.
    #[error("llm auth rejected: {0}")]
    Auth(String),
    /// The provider account is out of credit or quota. Terminal for the
    /// call: the fallback contract treats an exhausted account as a stop,
    /// not a retry.
    #[error("llm quota exhausted: {0}")]
    Quota(String),
    /// Every chain entry failed with a retryable error (transport, rate
    /// limit, other HTTP failure, or a malformed 2xx envelope); the message
    /// carries the last failure.
    #[error("llm unreachable: {0}")]
    Unreachable(String),
}

/// One use case's resolved primary endpoint: the provider, the model, the
/// base URL, and the key. The agent sidecar's init envelope carries the
/// same endpoint in band; nothing else may read the key.
pub struct LlmEndpoint {
    /// The provider id, for example `openai` or `anthropic`.
    pub provider: String,
    /// The provider's model name.
    pub model: String,
    /// The endpoint base URL, override or protocol default.
    pub base_url: String,
    /// The provider key.
    pub api_key: String,
}

impl std::fmt::Debug for LlmEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The key is deliberately absent: it is never printed.
        f.debug_struct("LlmEndpoint")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

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

/// One attempt's classified outcome.
enum Attempt {
    /// 2xx with text content; the ledger row is written.
    Done(String),
    /// An auth or quota rejection: the row is written and the call ends.
    Terminal(LlmError),
    /// A retryable failure carrying the message (for the exhausted tail) and
    /// the honoured `Retry-After` wait, when the response named one.
    Retry {
        message: String,
        wait: Option<Duration>,
    },
}

/// The background LLM client: the tier registry, the provider credentials
/// read once at startup, and the ledger every attempt lands in, over the
/// host's shared HTTP client.
pub struct LlmClient {
    /// The HTTP client the host built (with its TLS policy, as autodetection
    /// builds its fetcher).
    http: Client,
    /// The tier table use cases resolve their chains through.
    registry: TierTable,
    /// The ledger store.
    store: Arc<AiStore>,
    /// Provider API keys by provider name, read once at startup.
    keys: HashMap<String, String>,
    /// Endpoint base overrides by provider name, read once at startup.
    base_urls: HashMap<String, String>,
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The keys are deliberately absent: they are never printed.
        f.debug_struct("LlmClient")
            .field("registry", &self.registry)
            .field("providers", &self.keys.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl LlmClient {
    /// Builds the client over the host's HTTP client, the tier registry, and
    /// the ledger store, reading the provider credentials from the process
    /// environment once: `ASTRA_LLM_API_KEY_<PROVIDER>` and
    /// `ASTRA_LLM_BASE_URL_<PROVIDER>` with the provider uppercased, so
    /// `ASTRA_LLM_API_KEY_OPENAI` and `ASTRA_LLM_API_KEY_ANTHROPIC` (Task
    /// 1's `ASTRA_LLM_` prefix over inbox-zero's `<PROVIDER>_API_KEY` env
    /// shape; kylins keeps keys in encrypted settings rows, so there is no
    /// kylins env spelling to mirror). Keys live only in the map: never
    /// logged, never in a ledger row.
    pub fn new(http: Client, registry: TierTable, store: Arc<AiStore>) -> Self {
        Self::new_with_lookup(http, registry, store, |name| std::env::var(name).ok())
    }

    /// The seam behind [`LlmClient::new`]: `lookup` answers an environment
    /// variable name with its value, so tests inject credentials without
    /// racing the process environment.
    pub fn new_with_lookup(
        http: Client,
        registry: TierTable,
        store: Arc<AiStore>,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let mut keys = HashMap::new();
        let mut base_urls = HashMap::new();
        for provider in ["openai", "anthropic"] {
            let suffix = provider.to_ascii_uppercase();
            if let Some(key) = lookup(&format!("ASTRA_LLM_API_KEY_{suffix}")) {
                if !key.trim().is_empty() {
                    keys.insert(provider.to_string(), key);
                }
            }
            if let Some(base) = lookup(&format!("ASTRA_LLM_BASE_URL_{suffix}")) {
                if !base.trim().is_empty() {
                    base_urls.insert(provider.to_string(), base);
                }
            }
        }
        Self {
            http,
            registry,
            store,
            keys,
            base_urls,
        }
    }

    /// One non-streaming completion: resolves the use case's tier chain and
    /// attempts each entry in order, the first entry primary and the rest
    /// fallback. Retryable failures (transport errors, rate limits, other
    /// HTTP failures, malformed 2xx envelopes) advance the chain; auth and
    /// quota rejections are terminal for the call. Every HTTP attempt lands
    /// exactly one ledger row keyed by `correlation`. The returned string is
    /// the model's raw text; a structured caller parses it itself and
    /// retries once with `repair_json` on a bad parse. The streaming variant
    /// arrives with the A2 chat surface.
    ///
    /// # Errors
    /// [`LlmError::NotConfigured`] when no chain stands behind the use case
    /// or no entry of it is usable; [`LlmError::Auth`] and
    /// [`LlmError::Quota`] carry the terminal rejection; every retryable
    /// failure across the whole chain folds into [`LlmError::Unreachable`].
    pub async fn complete(
        &self,
        use_case: LlmUseCase,
        prompt: &str,
        correlation: &CorrelationId,
    ) -> Result<String, LlmError> {
        let chain = self
            .registry
            .resolve(use_case)
            .map_err(|e| LlmError::NotConfigured(e.to_string()))?;
        let mut attempted = 0usize;
        let mut last_failure = String::new();
        for (index, entry) in chain.iter().enumerate() {
            let Some(request) = self.request_for(entry, prompt) else {
                continue;
            };
            attempted += 1;
            match self.attempt(&request, entry, use_case, correlation).await {
                Attempt::Done(text) => return Ok(text),
                Attempt::Terminal(error) => return Err(error),
                Attempt::Retry { message, wait } => {
                    // A wait only serves an entry still to come: the chain's
                    // last entry skips it, no dead wait before the terminal
                    // error.
                    if let Some(wait) = wait.filter(|_| index + 1 < chain.len()) {
                        tokio::time::sleep(wait).await;
                    }
                    last_failure = message;
                }
            }
        }
        if attempted == 0 {
            return Err(LlmError::NotConfigured(format!(
                "no usable entry for use case {use_case}; set ASTRA_LLM_API_KEY_OPENAI or \
                 ASTRA_LLM_API_KEY_ANTHROPIC"
            )));
        }
        Err(LlmError::Unreachable(format!(
            "all {attempted} entries failed for use case {use_case}; last: {last_failure}"
        )))
    }

    /// The resolved primary endpoint for a use case: the first chain
    /// entry's provider, model, base URL, and key, as the agent sidecar's
    /// in-band model config delivery needs them. `None` when the use case
    /// resolves to no entry, the provider names no known protocol, or no
    /// key stands behind it: the same screening [`LlmClient::complete`]
    /// applies, so the sidecar is only configured with an endpoint the
    /// background client would have used itself.
    pub fn primary_endpoint(&self, use_case: LlmUseCase) -> Option<LlmEndpoint> {
        let chain = self.registry.resolve(use_case).ok()?;
        let entry = chain.iter().find(|entry| {
            wire::protocol_for(&entry.provider).is_some() && self.keys.contains_key(&entry.provider)
        })?;
        let protocol = wire::protocol_for(&entry.provider).expect("the entry was just screened");
        Some(LlmEndpoint {
            provider: entry.provider.clone(),
            model: entry.model.clone(),
            base_url: self
                .base_urls
                .get(&entry.provider)
                .cloned()
                .unwrap_or_else(|| protocol.default_base().to_string()),
            api_key: self.keys.get(&entry.provider).cloned()?,
        })
    }

    /// Builds the wire request for one chain entry, or `None` (logged, never
    /// carrying the key) when the provider names no protocol this build
    /// knows or no key stands behind it: the entry is skipped and the next
    /// one attempted, the kylins tier-resolution screening.
    fn request_for(&self, entry: &ModelEntry, prompt: &str) -> Option<WireRequest> {
        let Some(protocol) = wire::protocol_for(&entry.provider) else {
            warn!(
                "[llm] provider {} names no known protocol; skipping entry {}",
                entry.provider, entry.model
            );
            return None;
        };
        let Some(key) = self.keys.get(&entry.provider) else {
            warn!(
                "[llm] no api key for provider {}; skipping entry {}",
                entry.provider, entry.model
            );
            return None;
        };
        let base = self
            .base_urls
            .get(&entry.provider)
            .map_or_else(|| protocol.default_base().to_string(), String::clone);
        Some(wire::build_request(
            protocol,
            &base,
            &entry.model,
            prompt,
            key,
        ))
    }

    /// Sends one wire request, classifies the outcome, and lands exactly one
    /// ledger row: outcome and error class truthful, tokens and cost carried
    /// when the response parsed them. Non-2xx bodies are classified but
    /// never reported: an endpoint can echo the request's credentials back,
    /// and error strings are logged.
    async fn attempt(
        &self,
        request: &WireRequest,
        entry: &ModelEntry,
        use_case: LlmUseCase,
        correlation: &CorrelationId,
    ) -> Attempt {
        let protocol = wire::protocol_for(&entry.provider)
            .expect("the entry was screened when its request was built");
        let started = Instant::now();
        let mut send = self.http.post(&request.url).timeout(REQUEST_TIMEOUT);
        for (name, value) in &request.headers {
            send = send.header(*name, value.as_str());
        }
        let response = match send.body(request.body.clone()).send().await {
            Ok(response) => response,
            Err(e) => {
                let message = format!("{}: {e}", request.url);
                self.record(
                    entry,
                    use_case,
                    correlation,
                    started,
                    None,
                    "error:transport",
                );
                return Attempt::Retry {
                    message,
                    wait: None,
                };
            }
        };
        let status = response.status();
        let headers = response.headers().clone();
        let body = match response.text().await {
            Ok(body) => body,
            Err(e) => {
                let message = format!("{}: response body read failed: {e}", request.url);
                self.record(
                    entry,
                    use_case,
                    correlation,
                    started,
                    None,
                    "error:transport",
                );
                return Attempt::Retry {
                    message,
                    wait: None,
                };
            }
        };
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            let message = format!("{} returned HTTP {status}", request.url);
            self.record(entry, use_case, correlation, started, None, "error:auth");
            return Attempt::Terminal(LlmError::Auth(message));
        }
        if !status.is_success() {
            // The body is classified but never reported: a hostile or
            // mistyped endpoint can echo the request's credentials back in a
            // non-2xx body, and error strings are logged. Messages stay
            // status-level (the kylins precedent).
            return match classify_rejection(status.as_u16(), &body) {
                Rejection::Quota => {
                    let message = format!("{} returned HTTP {status}", request.url);
                    self.record(entry, use_case, correlation, started, None, "error:quota");
                    Attempt::Terminal(LlmError::Quota(message))
                }
                Rejection::RateLimited => {
                    let wait = retry_after(&headers);
                    let message = format!("{} returned HTTP {status}", request.url);
                    self.record(
                        entry,
                        use_case,
                        correlation,
                        started,
                        None,
                        "error:rate_limited",
                    );
                    Attempt::Retry { message, wait }
                }
                Rejection::Http => {
                    let message = format!("{} returned HTTP {status}", request.url);
                    self.record(
                        entry,
                        use_case,
                        correlation,
                        started,
                        None,
                        &format!("error:http_{}", status.as_u16()),
                    );
                    Attempt::Retry {
                        message,
                        wait: None,
                    }
                }
            };
        }
        match wire::parse_response(protocol, &body) {
            Ok(Completion { content, usage }) => {
                self.record(entry, use_case, correlation, started, Some(&usage), "ok");
                Attempt::Done(content)
            }
            Err(message) => {
                let message = format!("{}: {message}", request.url);
                self.record(
                    entry,
                    use_case,
                    correlation,
                    started,
                    None,
                    "error:bad_response",
                );
                Attempt::Retry {
                    message,
                    wait: None,
                }
            }
        }
    }

    /// Lands one ledger row for an attempt. A ledger failure is logged,
    /// never dropped silently, and never fails the call it records (the
    /// kylins record contract). Failure rows carry zero counters: only a
    /// parsed envelope states usage.
    fn record(
        &self,
        entry: &ModelEntry,
        use_case: LlmUseCase,
        correlation: &CorrelationId,
        started: Instant,
        usage: Option<&wire::Usage>,
        outcome: &str,
    ) {
        let row = LlmCallRow {
            correlation: correlation.clone(),
            use_case,
            tier: use_case.tier(),
            model: entry.model.clone(),
            prompt_tokens: usage.map_or(0, |usage| usage.prompt_tokens),
            completion_tokens: usage.map_or(0, |usage| usage.completion_tokens),
            cost_micros: usage.and_then(|usage| usage.cost_micros),
            latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            outcome: outcome.to_string(),
        };
        if let Err(e) = self.store.record_llm_call(&row) {
            warn!("[llm] ledger record failed: {e}");
        }
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "client_failure_tests.rs"]
mod failure_tests;
