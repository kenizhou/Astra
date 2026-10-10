// SPDX-License-Identifier: GPL-3.0-only
//! Seam tests against the one-shot HTTP fixture: the completion adapter
//! passes the model's text through and renders the host's failure as its
//! message, the judge adapter is infallible through a failing client (the
//! safe default answers), and the incoming-pass assembly arms only over a
//! client and a parseable DLP policy, staying unarmed when the policy is
//! malformed.

use std::sync::Arc;

use astra_ai::{CorrelationId, DlpError, DlpPolicy, JudgeUseCase, LlmUseCase};
use astra_app::{AiComplete, AiJudge};

use super::{incoming_pass, LlmAiComplete, LlmAiJudge};
use crate::ai::fixture::{self, env_at, llm_client, Ledger};
use crate::LlmClient;

/// One client over the fixture, recording into `ledger`, configured for the
/// default tier the seam tests' use cases resolve through.
fn client(ledger: &Ledger, base: &str) -> LlmClient {
    llm_client(
        &ledger.store(),
        &[("ASTRA_LLM_DEFAULT", "openai:gpt-4o-mini")],
        &env_at(base, &[("openai", "test-openai-key")]),
    )
}

/// The completion adapter passes the model's text through and lands the
/// ledger row under the caller's correlation.
#[test]
fn the_completion_adapter_passes_the_model_text_through() {
    let fixture = fixture::serve(vec![fixture::response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"choices":[{"message":{"role":"assistant","content":"two crisp rules"}}],"usage":{"prompt_tokens":9,"completion_tokens":5}}"#,
    )]);
    let ledger = Ledger::open("seam-complete");
    let seam = LlmAiComplete::new(Arc::new(client(&ledger, &fixture.base)));
    let correlation = CorrelationId::new("corr-seam-complete".to_string());

    let text = fixture::runtime()
        .block_on(|| seam.complete(LlmUseCase::PromptToRules, "Convert: hello", &correlation))
        .expect("the adapter passes the completion through");

    assert_eq!(text, "two crisp rules");
    assert_eq!(
        ledger.rows(),
        vec![fixture::row(
            "corr-seam-complete",
            "prompt_to_rules",
            "default",
            "gpt-4o-mini",
            (9, 5),
            None,
            "ok",
        )],
        "the adapter's call lands its ledger row",
    );
}

/// The completion adapter renders the host's failure as the seam's message
/// string: the app layer cannot name the host's error type and only reports
/// it.
#[test]
fn the_completion_adapter_renders_the_host_failure_as_its_message() {
    let fixture = fixture::serve(vec![fixture::response(
        "401 Unauthorized",
        &[("Content-Type", "application/json")],
        r#"{"error":"bad key"}"#,
    )]);
    let ledger = Ledger::open("seam-complete-fails");
    let seam = LlmAiComplete::new(Arc::new(client(&ledger, &fixture.base)));
    let correlation = CorrelationId::new("corr-seam-complete-fails".to_string());

    let failure = fixture::runtime()
        .block_on(|| seam.complete(LlmUseCase::PromptToRules, "Convert: hello", &correlation))
        .expect_err("the auth rejection crosses the completion seam as an error");

    assert!(
        failure.contains("llm auth rejected"),
        "the message is the host failure's own rendering: {failure}"
    );
}

/// The judge adapter never errors: a client that fails outright answers the
/// use case's safe default under the abstain-default path, exactly the
/// infallible-seam contract the app's matching paths rest on.
#[test]
fn the_judge_adapter_is_infallible_through_a_failing_client() {
    let fixture = fixture::serve(vec![fixture::response(
        "401 Unauthorized",
        &[("Content-Type", "application/json")],
        r#"{"error":"bad key"}"#,
    )]);
    let ledger = Ledger::open("seam-judge-fails");
    let seam = LlmAiJudge::new(Arc::new(client(&ledger, &fixture.base)));
    let correlation = CorrelationId::new("corr-seam-judge-fails".to_string());

    let outcome = fixture::runtime().block_on(|| {
        seam.judge(
            JudgeUseCase::JudgeColdEmail,
            "Is the message cold outreach?",
            "From: someone <someone@example.test>",
            &correlation,
        )
    });

    assert_eq!(
        outcome.verdict.as_str(),
        "no",
        "the cold default is not cold"
    );
    assert_eq!(
        outcome.path.as_str(),
        "abstain_default",
        "a failed call resolves through the safe-default table"
    );
    assert_eq!(
        ledger.rows(),
        vec![fixture::row(
            "corr-seam-judge-fails",
            "judge_cold_email",
            "nano",
            "gpt-4o-mini",
            (0, 0),
            None,
            "error:auth",
        )],
        "the failed attempt's row landed before the default applied",
    );
}

/// The pass assembly arms over a client and a parseable policy, and stays
/// unarmed when either half is missing: no client (the background client
/// never built) or a malformed DLP value, the loud refusal rather than a
/// silent allow. The draft-pool sink is an optional rider: the pass arms
/// the same with one as without.
#[test]
fn the_pass_assembly_arms_only_over_a_client_and_a_parseable_policy() {
    let fixture = fixture::serve(Vec::new());
    let ledger = Ledger::open("seam-pass");
    let built = Arc::new(client(&ledger, &fixture.base));

    assert!(
        incoming_pass(
            Some(Arc::clone(&built)),
            ledger.store(),
            Ok(DlpPolicy::Redact),
            None
        )
        .is_some(),
        "a client and a parseable policy arm the pass"
    );
    assert!(
        incoming_pass(None, ledger.store(), Ok(DlpPolicy::Allow), None).is_none(),
        "no client means no pass"
    );
    let malformed = DlpError::Malformed {
        value: "sometimes".to_owned(),
    };
    assert!(
        incoming_pass(
            Some(Arc::clone(&built)),
            ledger.store(),
            Err(malformed),
            None
        )
        .is_none(),
        "a malformed policy leaves the pass unarmed, whatever the client"
    );
}
