// SPDX-License-Identifier: GPL-3.0-only
//! Failure-class client tests over the one-shot HTTP fixture: the terminal
//! error classes (quota, auth) with status-level messages that never carry a
//! response body, the exhausted chain, the dead-wait skip on a rate-limited
//! last entry, the malformed-envelope advance, and the entry screening (no
//! key, unknown provider). The wire-shape success paths and the pure
//! classification helpers live in `client_tests`; the fixture, client
//! builders, and ledger reader in `fixture`.

use std::time::Duration;

use astra_ai::{CorrelationId, LlmUseCase};

use super::LlmError;
use crate::ai::fixture::{self, env_at, llm_client, row, runtime, Ledger};

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
        matches!(error, LlmError::Quota(ref message) if message.contains("429") && !message.contains("k1")),
        "the error is typed quota, status-level, and carries no key: {error:?}",
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
        matches!(error, LlmError::Auth(ref message) if message.contains("401") && !message.contains("a-bad-key")),
        "the error is typed auth, status-level, and carries no key: {error:?}",
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
fn response_bodies_never_flow_into_error_strings_or_rows() {
    // A hostile or mistyped endpoint can echo the authorization value back in
    // a non-2xx body, so the body is classified but never reported.
    let key = "k1-secret-openai";
    let echoed = format!("insufficient_quota: the key {key} has no credit left");
    let fixture = fixture::serve(vec![fixture::response(
        "402 Payment Required",
        &[("Content-Type", "text/plain")],
        &echoed,
    )]);
    let ledger = Ledger::open("key-echo");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(&fixture.base, &[("openai", key)]),
    );
    let correlation = CorrelationId::new("corr-echo".to_string());

    let error = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect_err("the echoed quota body still classifies as quota");

    assert!(
        matches!(error, LlmError::Quota(ref message) if message.contains("402") && !message.contains(key)),
        "the error carries the status and never the echoed key: {error:?}",
    );
    assert_eq!(
        fixture.request(0).header("authorization"),
        format!("Bearer {key}"),
        "the key was on the wire, so the leak the test guards was real",
    );
    for recorded in ledger.rows() {
        assert!(
            !format!("{recorded:?}").contains(key),
            "no ledger field carries the key: {recorded:?}",
        );
    }
}

#[test]
fn an_exhausted_chain_folds_into_unreachable_with_a_row_per_attempt() {
    let fixture = fixture::serve(vec![
        fixture::response(
            "429 Too Many Requests",
            &[("Content-Type", "application/json")],
            r#"{"error":{"type":"rate_limit_error","message":"Rate limit reached"}}"#,
        ),
        fixture::response("503 Service Unavailable", &[], "upstream melted"),
    ]);
    let ledger = Ledger::open("exhausted");
    let client = llm_client(
        &ledger.store(),
        &[(
            "ASTRA_LLM_ECONOMY",
            "openai:gpt-4o-mini,anthropic:claude-3-5-haiku-latest",
        )],
        &env_at(&fixture.base, &[("openai", "k1"), ("anthropic", "k2")]),
    );
    let correlation = CorrelationId::new("corr-exhausted".to_string());

    let error = runtime()
        .block_on(|| client.complete(LlmUseCase::AgentTool, "Triage", &correlation))
        .expect_err("a chain with no working entry is unreachable");

    assert!(
        matches!(error, LlmError::Unreachable(ref message) if message.contains("2 entries") && message.contains("503")),
        "the error names the exhausted chain and its last failure: {error:?}",
    );
    assert_eq!(fixture.request_count(), 2, "every entry was attempted");
    assert_eq!(
        fixture.request(0).request_line,
        "POST /chat/completions HTTP/1.1",
        "the primary 429 without Retry-After still advanced",
    );
    assert_eq!(
        fixture.request(1).request_line,
        "POST /v1/messages HTTP/1.1",
        "the fallback got its attempt before the fold",
    );
    assert_eq!(
        ledger.rows(),
        vec![
            row(
                "corr-exhausted",
                "agent_tool",
                "economy",
                "gpt-4o-mini",
                (0, 0),
                None,
                "error:rate_limited",
            ),
            row(
                "corr-exhausted",
                "agent_tool",
                "economy",
                "claude-3-5-haiku-latest",
                (0, 0),
                None,
                "error:http_503",
            ),
        ],
    );
}

#[test]
fn a_rate_limited_last_entry_returns_without_the_dead_wait() {
    let fixture = fixture::serve(vec![fixture::response(
        "429 Too Many Requests",
        &[("Retry-After", "2")],
        r#"{"error":{"type":"rate_limit_error"}}"#,
    )]);
    let ledger = Ledger::open("dead-wait");
    let client = llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(&fixture.base, &[("openai", "k1")]),
    );
    let correlation = CorrelationId::new("corr-deadwait".to_string());

    let started = std::time::Instant::now();
    let error = runtime()
        .block_on(|| client.complete(LlmUseCase::Summarise, "Summarise", &correlation))
        .expect_err("a lone rate-limited entry is unreachable");

    assert!(
        matches!(error, LlmError::Unreachable(ref message) if message.contains("429")),
        "the error is typed unreachable: {error:?}",
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the last entry's Retry-After word is not waited out: {:?}",
        started.elapsed(),
    );
    assert_eq!(fixture.request_count(), 1);
    assert_eq!(
        ledger.rows(),
        vec![row(
            "corr-deadwait",
            "summarise",
            "default",
            "gpt-4o-mini",
            (0, 0),
            None,
            "error:rate_limited",
        )],
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
