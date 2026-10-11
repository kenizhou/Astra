// SPDX-License-Identifier: GPL-3.0-only
//! Report service tests, over the fixture completion seam: the
//! deterministic statistics computed before the one call (the counts
//! per category, the unread rate, the sent count), the three sections
//! assembled with their headings, the listings riding their own
//! untrusted wraps, the days clamp, the typed failure paths with the
//! repair path, and the version pin.

use astra_ai::{CorrelationId, LlmUseCase};
use engine_api::{AccountId, ProviderKey};

use super::report::{generate_report, report_prompt, ReportError, REPORT_PROMPT_VERSION};
use super::{DocMessage, StoredCategories, DOCUMENT_WINDOW_MAX_DAYS};
use crate::dispatch_fakes::ai::FixtureComplete;

/// The account whose window the tests report over.
fn account() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// One window message: `at` naming its position in the newest-first
/// list the caller collected, `unread` the flag the statistics count.
fn message(at: usize, unread: bool) -> DocMessage {
    DocMessage {
        key: ProviderKey::new(format!("m{at}")).unwrap(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        subject: format!("Note {at}"),
        body: format!("Body {at}: the launch lands on Friday the {at}th."),
        unread,
    }
}

/// The stored categorisation the statistics ride over, straight from
/// the caller's rows: message keys beside their stored category words.
fn stored(pairs: &[(&str, &str)]) -> StoredCategories {
    pairs
        .iter()
        .map(|(key, category)| (ProviderKey::new(*key).unwrap(), (*category).to_owned()))
        .collect()
}

/// The good fixture: the three-section shape the output policy pins.
const GOOD: &str = r#"{"executive_summary":"A founder's mail.",
    "response_patterns":"Mornings bring the mail that matters.",
    "recommendations":"Answer the two primary notes first."}"#;

/// The assembled document the good fixture answers.
const ASSEMBLED: &str = "Executive summary\nA founder's mail.\n\
     \n\
     Response patterns\nMornings bring the mail that matters.\n\
     \n\
     Recommendations\nAnswer the two primary notes first.";

#[tokio::test]
async fn a_report_computes_its_stats_then_generates_once() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let correlation = CorrelationId::generate();
    let messages = vec![
        message(0, true),
        message(1, false),
        message(2, false),
        message(3, false),
    ];
    let stored_categories = stored(&[
        ("m0", "primary"),
        ("m1", "primary"),
        ("m2", "newsletter"),
        ("m3", "newsletter"),
    ]);
    let report = generate_report(
        &account(),
        7,
        &messages,
        &stored_categories,
        4,
        &correlation,
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(
        report, ASSEMBLED,
        "the three sections assemble under their headings",
    );
    assert_eq!(complete.call_count(), 1, "the report is one call");
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::Report);
    assert_eq!(call.correlation, correlation);
    assert!(
        call.prompt.contains("messages in the window: 4")
            && call.prompt.contains("unread: 1 of 4 (25%)")
            && call.prompt.contains("sent by the user in the window: 4")
            && call.prompt.contains("primary: 2")
            && call.prompt.contains("newsletter: 2"),
        "the statistics computed before the call ride the prompt:\n{}",
        call.prompt,
    );
    assert_eq!(REPORT_PROMPT_VERSION, "1");
}

#[test]
fn the_unread_rate_floors_to_whole_percent() {
    let messages = vec![message(0, true), message(1, false), message(2, false)];
    let prompt = report_prompt(7, &messages, &stored(&[]), 0);
    assert!(
        prompt.contains("unread: 1 of 3 (33%)"),
        "a fractional rate floors to its whole percent:\n{prompt}",
    );
}

#[test]
fn each_listing_message_rides_its_own_untrusted_wrap() {
    let messages = vec![message(0, false), message(1, false)];
    let stored_categories = stored(&[("m0", "primary"), ("m1", "cold")]);
    let prompt = report_prompt(7, &messages, &stored_categories, 2);
    assert_eq!(
        prompt.matches("<untrusted heading=\"email\">").count(),
        2,
        "each listing message rides its own untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("Category primary") && prompt.contains("Category cold"),
        "the category lines ride the trusted half outside the wraps:\n{prompt}",
    );
}

#[test]
fn the_days_clamp_pins_one_to_thirty_one() {
    assert_eq!(
        DOCUMENT_WINDOW_MAX_DAYS, 31,
        "the ceiling is the spec's pin",
    );
    let messages = [message(0, false)];
    assert!(
        report_prompt(0, &messages, &stored(&[]), 0).contains("last 1 day"),
        "a zero-day ask clamps up to one",
    );
    assert!(
        report_prompt(40, &messages, &stored(&[]), 0).contains("last 31 days"),
        "an over-ceiling ask clamps down to thirty-one",
    );
}

#[tokio::test]
async fn an_empty_window_makes_no_call_and_answers_empty() {
    let complete = FixtureComplete::scripted(&[]);
    let report = generate_report(
        &account(),
        7,
        &[],
        &stored(&[]),
        0,
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(report, "");
    assert_eq!(complete.call_count(), 0, "no messages, no call");
}

#[tokio::test]
async fn a_failed_call_answers_the_typed_completion_error() {
    let complete = FixtureComplete::failing("llm unreachable: every attempt failed");
    let error = generate_report(
        &account(),
        7,
        &[message(0, false)],
        &stored(&[]),
        2,
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap_err();
    match &error {
        ReportError::Completion(detail) => {
            assert_eq!(detail, "llm unreachable: every attempt failed");
        }
        ReportError::MalformedOutput(detail) => {
            panic!("the failure is the completion error, not a parse failure: {detail}");
        }
    }
}

#[tokio::test]
async fn unparseable_output_answers_the_typed_malformed_error() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let error = generate_report(
        &account(),
        7,
        &[message(0, false)],
        &stored(&[]),
        2,
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, ReportError::MalformedOutput(_)),
        "the failure is the malformed-output error",
    );
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, the report:\n{GOOD}\nHope that helps!");
    let complete = FixtureComplete::scripted(&[&answer]);
    let report = generate_report(
        &account(),
        7,
        &[message(0, false)],
        &stored(&[]),
        2,
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(report, ASSEMBLED, "the repaired completion still assembles");
}
