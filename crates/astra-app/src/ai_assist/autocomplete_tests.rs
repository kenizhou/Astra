// SPDX-License-Identifier: GPL-3.0-only
//! Autocomplete tests, over the fixture completion seam: the suggestion
//! path (the Autocomplete use case under the turn's correlation, the
//! trimmed answer), the prompt pins (both cursor halves each in their own
//! untrusted wrap, the absent trailing half omitted, the length cap in
//! the instruction, the version), and the failure path (an empty
//! suggestion, never a crash).

use astra_ai::{CorrelationId, LlmUseCase};

use super::autocomplete::{autocomplete, autocomplete_prompt, AUTOCOMPLETE_VERSION};
use crate::dispatch_fakes::ai::FixtureComplete;

#[tokio::test]
async fn a_suggestion_rides_the_autocomplete_use_case() {
    let complete = FixtureComplete::scripted(&["  Friday morning.  "]);
    let correlation = CorrelationId::generate();
    let suggestion = autocomplete(
        "The launch is on",
        "\n\nKind regards",
        &correlation,
        &complete,
    )
    .await;
    assert_eq!(suggestion, "Friday morning.", "the answer rides trimmed");
    assert_eq!(complete.call_count(), 1);
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::Autocomplete);
    assert_eq!(call.correlation, correlation);
}

#[tokio::test]
async fn a_failed_call_answers_an_empty_string() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let suggestion = autocomplete(
        "The launch is on",
        "",
        &CorrelationId::generate(),
        &complete,
    )
    .await;
    assert_eq!(suggestion, "", "the failure answers nothing, no crash");
}

#[tokio::test]
async fn a_whitespace_answer_trims_to_an_empty_suggestion() {
    let complete = FixtureComplete::scripted(&["   \n\t "]);
    let suggestion = autocomplete(
        "The launch is on",
        "",
        &CorrelationId::generate(),
        &complete,
    )
    .await;
    assert_eq!(suggestion, "");
}

#[test]
fn the_prompt_carries_both_halves_wrapped_and_the_length_cap() {
    let prompt = autocomplete_prompt("The launch is on", "\n\nKind regards");
    assert!(
        prompt.contains("<untrusted heading=\"editor text before the cursor\">"),
        "the text before the cursor rides its own untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("<untrusted heading=\"editor text after the cursor\">"),
        "the trailing context rides its own untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("The launch is on") && prompt.contains("Kind regards"),
        "both halves ride:\n{prompt}",
    );
    assert!(
        prompt.contains("no more than about ten words"),
        "the continuation instruction carries its length cap:\n{prompt}",
    );
    assert_eq!(AUTOCOMPLETE_VERSION, "1");
}

#[test]
fn an_absent_trailing_half_omits_its_block() {
    let prompt = autocomplete_prompt("The launch is on", "");
    assert!(
        !prompt.contains("<untrusted heading=\"editor text after the cursor\">"),
        "the optional block never rides empty:\n{prompt}",
    );
    assert!(
        prompt.contains("<untrusted heading=\"editor text before the cursor\">"),
        "the leading half still rides:\n{prompt}",
    );
}

#[test]
fn a_whitespace_trailing_half_omits_its_block() {
    let prompt = autocomplete_prompt("The launch is on", "   ");
    assert!(
        !prompt.contains("<untrusted heading=\"editor text after the cursor\">"),
        "a whitespace-only trailing half is absent:\n{prompt}",
    );
}
