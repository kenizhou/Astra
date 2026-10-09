// SPDX-License-Identifier: GPL-3.0-only
//! Client tests against the one-shot HTTP fixture: the dual-protocol wire
//! shapes pinned byte-relevantly from the fixture's capture, the fallback and
//! terminal error classes with their ledger rows, the env spellings read at
//! startup, and the pure classification helpers (rejection classes,
//! Retry-After). The fixture, the client builders, and the ledger reader live
//! in the sibling `fixture` module; the repair_json cases in `wire_tests`.

use std::time::Duration;

use astra_ai::{CorrelationId, LlmUseCase};
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

use super::{classify_rejection, retry_after, LlmClient, LlmError, Rejection};
use crate::ai::fixture::{self, env_at, llm_client, row, runtime, Ledger};

#[test]
fn openai_success_pins_the_wire_shape_and_lands_one_ok_row() {
    let fixture = fixture::serve(vec![fixture::response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"choices":[{"message":{"role":"assistant","content":"a crisp summary"}}],"usage":{"prompt_tokens":11,"completion_tokens":22}}"#,
    )]);
    let ledger = Ledger::open("openai-success");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(&fixture.base, &[("openai", "test-openai-key")]),
    );
    let correlation = CorrelationId::new("corr-openai".to_string());

    let text = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise: hello", &correlation))
        .expect("the openai entry completes");

    assert_eq!(text, "a crisp summary");
    let request = fixture.request(0);
    assert_eq!(request.request_line, "POST /chat/completions HTTP/1.1");
    assert_eq!(request.header("authorization"), "Bearer test-openai-key");
    assert_eq!(request.header("content-type"), "application/json");
    assert_eq!(
        request.body,
        r#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Summarise: hello"}]}"#,
        "the openai wire shape is byte-exact",
    );
    assert_eq!(fixture.request_count(), 1, "exactly one HTTP attempt");
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-openai",
            "summarise",
            "default",
            "gpt-4o-mini",
            (11, 22),
            None,
            "ok",
        )],
    );
}

#[test]
fn anthropic_success_pins_the_wire_shape_and_lands_one_ok_row() {
    let fixture = fixture::serve(vec![fixture::response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"content":[{"type":"text","text":"a haiku-grade reply"}],"usage":{"input_tokens":7,"output_tokens":9}}"#,
    )]);
    let ledger = Ledger::open("anthropic-success");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_CHAT", "anthropic:claude-3-5-haiku-latest")],
        &env_at(&fixture.base, &[("anthropic", "test-anthropic-key")]),
    );
    let correlation = CorrelationId::new("corr-anthropic".to_string());

    let text = runtime()
        .block_on(|| client.complete(LlmUseCase::AssistantChat, "Draft: thanks", &correlation))
        .expect("the anthropic entry completes");

    assert_eq!(text, "a haiku-grade reply");
    let request = fixture.request(0);
    assert_eq!(request.request_line, "POST /v1/messages HTTP/1.1");
    assert_eq!(request.header("x-api-key"), "test-anthropic-key");
    assert_eq!(request.header("anthropic-version"), "2023-06-01");
    assert_eq!(request.header("content-type"), "application/json");
    assert_eq!(
        request.body,
        r#"{"model":"claude-3-5-haiku-latest","max_tokens":1024,"messages":[{"role":"user","content":"Draft: thanks"}]}"#,
        "the anthropic wire shape is byte-exact",
    );
    assert_eq!(fixture.request_count(), 1, "exactly one HTTP attempt");
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-anthropic",
            "assistant_chat",
            "chat",
            "claude-3-5-haiku-latest",
            (7, 9),
            None,
            "ok",
        )],
    );
}

#[test]
fn a_retryable_primary_falls_through_to_the_fallback_entry() {
    let fixture = fixture::serve(vec![
        fixture::response(
            "429 Too Many Requests",
            &[("Retry-After", "0"), ("Content-Type", "application/json")],
            r#"{"error":{"type":"rate_limit_error","message":"Rate limit reached"}}"#,
        ),
        fixture::response(
            "200 OK",
            &[("Content-Type", "application/json")],
            r#"{"content":[{"type":"text","text":"from the fallback"}],"usage":{"input_tokens":3,"output_tokens":4}}"#,
        ),
    ]);
    let ledger = Ledger::open("fallback");
    let client = llm_client(
        &ledger.store(),
        &[(
            "ASTRA_LLM_ECONOMY",
            "openai:gpt-4o-mini,anthropic:claude-3-5-haiku-latest",
        )],
        &env_at(&fixture.base, &[("openai", "k1"), ("anthropic", "k2")]),
    );
    let correlation = CorrelationId::new("corr-fallback".to_string());

    let text = runtime()
        .block_on(|| client.complete(LlmUseCase::AgentTool, "Triage this message", &correlation))
        .expect("the fallback entry completes");

    assert_eq!(text, "from the fallback");
    assert_eq!(fixture.request_count(), 2, "one attempt per chain entry");
    assert_eq!(
        fixture.request(0).request_line,
        "POST /chat/completions HTTP/1.1",
        "the primary attempt rode openai",
    );
    assert_eq!(
        fixture.request(1).request_line,
        "POST /v1/messages HTTP/1.1",
        "the chain advanced to the anthropic fallback",
    );
    assert_eq!(
        ledger.rows(),
        vec![
            row(
                "corr-fallback",
                "agent_tool",
                "economy",
                "gpt-4o-mini",
                (0, 0),
                None,
                "error:rate_limited",
            ),
            row(
                "corr-fallback",
                "agent_tool",
                "economy",
                "claude-3-5-haiku-latest",
                (3, 4),
                None,
                "ok",
            ),
        ],
        "one row per attempt, the chain advanced inside the tier's list",
    );
}

#[test]
fn a_quota_rejection_is_terminal_with_one_row_and_a_typed_error() {
    let fixture = fixture::serve(vec![fixture::response(
        "429 Too Many Requests",
        &[("Content-Type", "application/json")],
        r#"{"error":{"type":"insufficient_quota","message":"You exceeded your current quota, please check your plan and billing details."}}"#,
    )]);
    let ledger = Ledger::open("quota");
    let client = llm_client(
        &ledger.store(),
        &[(
            "ASTRA_LLM_ECONOMY",
            "openai:gpt-4o-mini,anthropic:claude-3-5-haiku-latest",
        )],
        &env_at(&fixture.base, &[("openai", "k1"), ("anthropic", "k2")]),
    );
    let correlation = CorrelationId::new("corr-quota".to_string());

    let error = runtime()
        .block_on(|| client.complete(LlmUseCase::AgentTool, "Triage", &correlation))
        .expect_err("an exhausted quota is terminal for the call");

    assert!(
        matches!(error, LlmError::Quota(ref message) if message.contains("insufficient_quota")),
        "the error is typed quota: {error:?}",
    );
    assert_eq!(
        fixture.request_count(),
        1,
        "quota never reaches the fallback entry",
    );
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-quota",
            "agent_tool",
            "economy",
            "gpt-4o-mini",
            (0, 0),
            None,
            "error:quota",
        )],
    );
}

#[test]
fn an_auth_rejection_is_terminal_for_the_call() {
    let fixture = fixture::serve(vec![fixture::response(
        "401 Unauthorized",
        &[("Content-Type", "application/json")],
        r#"{"error":{"message":"Incorrect API key provided"}}"#,
    )]);
    let ledger = Ledger::open("auth");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(&fixture.base, &[("openai", "a-bad-key")]),
    );
    let correlation = CorrelationId::new("corr-auth".to_string());

    let error = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect_err("a rejected credential is terminal for the call");

    assert!(
        matches!(error, LlmError::Auth(_)),
        "the error is typed auth: {error:?}",
    );
    assert_eq!(fixture.request_count(), 1);
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-auth",
            "summarise",
            "default",
            "gpt-4o-mini",
            (0, 0),
            None,
            "error:auth",
        )],
    );
}

#[test]
fn the_cost_field_is_parsed_when_the_response_carries_one() {
    let fixture = fixture::serve(vec![fixture::response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"choices":[{"message":{"content":"priced"}}],"usage":{"prompt_tokens":5,"completion_tokens":6,"cost":0.000123}}"#,
    )]);
    let ledger = Ledger::open("cost");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(&fixture.base, &[("openai", "test-openai-key")]),
    );
    let correlation = CorrelationId::new("corr-cost".to_string());

    let text = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect("the cost field does not disturb the completion");

    assert_eq!(text, "priced");
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-cost",
            "summarise",
            "default",
            "gpt-4o-mini",
            (5, 6),
            Some(123),
            "ok",
        )],
        "the OpenRouter-style usage cost lands as micros",
    );
}

#[test]
fn a_malformed_two_xx_envelope_advances_the_chain() {
    let fixture = fixture::serve(vec![
        fixture::response("200 OK", &[("Content-Type", "text/plain")], "just words"),
        fixture::response(
            "200 OK",
            &[("Content-Type", "application/json")],
            r#"{"choices":[{"message":{"content":"recovered"}}],"usage":{"prompt_tokens":1,"completion_tokens":2}}"#,
        ),
    ]);
    let ledger = Ledger::open("bad-envelope");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini,openai:gpt-4o")],
        &env_at(&fixture.base, &[("openai", "test-openai-key")]),
    );
    let correlation = CorrelationId::new("corr-envelope".to_string());

    let text = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect("the second entry answers");

    assert_eq!(text, "recovered");
    assert_eq!(
        ledger.rows(),
        vec![
            row(
                "corr-envelope",
                "summarise",
                "default",
                "gpt-4o-mini",
                (0, 0),
                None,
                "error:bad_response",
            ),
            row(
                "corr-envelope",
                "summarise",
                "default",
                "gpt-4o",
                (1, 2),
                None,
                "ok",
            ),
        ],
    );
}

#[test]
fn an_entry_without_a_key_is_not_configured_and_writes_nothing() {
    let fixture = fixture::serve(Vec::new());
    let ledger = Ledger::open("no-key");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(&fixture.base, &[]),
    );
    let correlation = CorrelationId::new("corr-nokey".to_string());

    let error = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect_err("no key stands behind the only entry");

    assert!(
        matches!(error, LlmError::NotConfigured(ref message) if message.contains("ASTRA_LLM_API_KEY")),
        "the error names the env spelling: {error:?}",
    );
    assert_eq!(fixture.request_count(), 0, "no HTTP attempt was made");
    assert!(ledger.rows().is_empty(), "no row lands without an attempt");
}

#[test]
fn an_unknown_provider_is_skipped_and_the_chain_continues() {
    let fixture = fixture::serve(vec![fixture::response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"choices":[{"message":{"content":"answered"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#,
    )]);
    let ledger = Ledger::open("unknown-provider");
    let client = llm_client(
        &ledger.store(),
        &[(
            "ASTRA_LLM_DEFAULT",
            "groq:llama-3.1-8b-instant,openai:gpt-4o-mini",
        )],
        &env_at(&fixture.base, &[("openai", "test-openai-key")]),
    );
    let correlation = CorrelationId::new("corr-skip".to_string());

    let text = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect("the openai entry behind the unknown provider completes");

    assert_eq!(text, "answered");
    assert_eq!(fixture.request_count(), 1, "only the openai entry attempts");
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-skip",
            "summarise",
            "default",
            "gpt-4o-mini",
            (1, 1),
            None,
            "ok",
        )],
    );
}

#[test]
fn startup_reads_provider_keys_and_base_overrides_from_the_env_once() {
    let ledger = Ledger::open("startup-env");
    let asked: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    LlmClient::new_with_lookup(
        fixture::http(),
        fixture::tier_table(&[]),
        ledger.store(),
        |name| {
            asked
                .lock()
                .expect("the env capture is unpoisoned")
                .push(name.to_string());
            None
        },
    );

    let asked = asked.into_inner().expect("the env capture is unpoisoned");
    for name in [
        "ASTRA_LLM_API_KEY_OPENAI",
        "ASTRA_LLM_API_KEY_ANTHROPIC",
        "ASTRA_LLM_BASE_URL_OPENAI",
        "ASTRA_LLM_BASE_URL_ANTHROPIC",
    ] {
        assert!(
            asked.contains(&name.to_string()),
            "startup reads {name}; asked: {asked:?}",
        );
    }
}

#[test]
fn rejection_classification_pins_the_quota_and_rate_limit_boundary() {
    assert!(matches!(classify_rejection(402, ""), Rejection::Quota));
    assert!(matches!(
        classify_rejection(429, r#"{"error":{"type":"insufficient_quota"}}"#),
        Rejection::Quota
    ));
    assert!(matches!(
        classify_rejection(
            400,
            "Your credit balance is too low to access the Anthropic API"
        ),
        Rejection::Quota
    ));
    assert!(matches!(
        classify_rejection(429, r#"{"error":{"type":"rate_limit_error"}}"#),
        Rejection::RateLimited
    ));
    assert!(matches!(
        classify_rejection(503, "upstream melted"),
        Rejection::Http
    ));
    assert!(matches!(
        classify_rejection(404, "model not found"),
        Rejection::Http
    ));
}

#[test]
fn retry_after_is_parsed_in_delta_seconds_and_capped() {
    let headers = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_str(value).unwrap());
        headers
    };

    assert_eq!(retry_after(&headers("0")), Some(Duration::ZERO));
    assert_eq!(
        retry_after(&headers("120")),
        Some(Duration::from_secs(2)),
        "the cap bounds a hostile Retry-After",
    );
    assert_eq!(
        retry_after(&headers("Wed, 21 Oct 2026 07:28:00 GMT")),
        None,
        "an HTTP-date reads as absent",
    );
    assert_eq!(retry_after(&HeaderMap::new()), None);
}
