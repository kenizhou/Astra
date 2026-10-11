// SPDX-License-Identifier: GPL-3.0-only
//! Digest service tests, over the fixture completion seam: the one
//! generation (the Digest use case under the turn's correlation), the
//! rollup from the stored categories with the uncategorised bucket and
//! foreign words under their own word, the per-category head-five with
//! the true count still rolled up, each message in its own untrusted
//! wrap, the days clamp, the typed failure paths with the repair path,
//! and the version pin.

use astra_ai::{CorrelationId, LlmUseCase};
use engine_api::{AccountId, ProviderKey};

use super::digest::{digest_prompt, generate_digest, DigestError, DIGEST_PROMPT_VERSION};
use super::{
    DocMessage, StoredCategories, DOCUMENT_BODY_CAP, DOCUMENT_HEAD_LIMIT, DOCUMENT_WINDOW_MAX_DAYS,
};
use crate::dispatch_fakes::ai::FixtureComplete;

/// The account whose window the tests digest.
fn account() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// One window message: `at` naming its position in the newest-first
/// list the caller collected, `unread` the flag the report counts.
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

/// The stored categorisation the rollup rides over, straight from the
/// caller's rows: message keys beside their stored category words.
fn stored(pairs: &[(&str, &str)]) -> StoredCategories {
    pairs
        .iter()
        .map(|(key, category)| (ProviderKey::new(*key).unwrap(), (*category).to_owned()))
        .collect()
}

/// The good fixture: the one-field digest shape the output policy pins.
const GOOD: &str = r#"{"digest":"Two notes need replies; the newsletter can wait."}"#;

#[tokio::test]
async fn a_digest_generates_from_one_call_over_the_rollup() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let correlation = CorrelationId::generate();
    let messages = vec![message(0, false), message(1, true), message(2, false)];
    let stored_categories = stored(&[("m0", "primary"), ("m1", "primary"), ("m2", "newsletter")]);
    let digest = generate_digest(
        &account(),
        7,
        &messages,
        &stored_categories,
        &correlation,
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(digest, "Two notes need replies; the newsletter can wait.");
    assert_eq!(complete.call_count(), 1, "the digest is one call");
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::Digest);
    assert_eq!(call.correlation, correlation);
    assert!(
        call.prompt.contains("primary: 2") && call.prompt.contains("newsletter: 1"),
        "the rollup counts every message by its stored category:\n{}",
        call.prompt,
    );
    assert_eq!(DIGEST_PROMPT_VERSION, "1");
}

#[test]
fn a_message_without_a_stored_category_lands_in_the_uncategorised_bucket() {
    let prompt = digest_prompt(7, &[message(0, false)], &stored(&[]));
    assert!(
        prompt.contains("uncategorised: 1"),
        "the rollup names the uncategorised bucket:\n{prompt}",
    );
    assert!(
        prompt.contains("Category uncategorised"),
        "the grouping carries the uncategorised bucket's messages:\n{prompt}",
    );
}

#[test]
fn a_foreign_stored_word_rolls_up_under_its_own_word() {
    let messages = [message(0, false), message(1, false)];
    let stored_categories = stored(&[("m0", "rumour"), ("m1", "primary")]);
    let prompt = digest_prompt(7, &messages, &stored_categories);
    let known = prompt
        .find("primary: 1")
        .expect("the closed set's word counts");
    let foreign = prompt
        .find("rumour: 1")
        .expect("the foreign word counts under itself");
    assert!(
        prompt.find("uncategorised: 1").is_none(),
        "a stored word is stored, never bucketed as absent:\n{prompt}",
    );
    assert!(
        known < foreign,
        "the closed set's order leads, foreign words follow:\n{prompt}",
    );
}

#[test]
fn each_message_rides_its_own_untrusted_wrap() {
    let messages = vec![message(0, false), message(1, false)];
    let stored_categories = stored(&[("m0", "primary"), ("m1", "newsletter")]);
    let prompt = digest_prompt(7, &messages, &stored_categories);
    assert_eq!(
        prompt.matches("<untrusted heading=\"email\">").count(),
        2,
        "each message rides its own untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("Category primary") && prompt.contains("Category newsletter"),
        "the category lines ride the trusted half outside the wraps:\n{prompt}",
    );
    assert!(
        prompt.contains("From: Ada Lovelace") && prompt.contains("Subject: Note 0"),
        "the rendering carries the sender and subject:\n{prompt}",
    );
}

#[test]
fn the_head_five_cap_keeps_the_newest_five_per_category() {
    assert_eq!(DOCUMENT_HEAD_LIMIT, 5, "the head cap is the spec's pin");
    let messages: Vec<DocMessage> = (0..7).map(|at| message(at, false)).collect();
    let stored_categories: StoredCategories = (0..7)
        .map(|at| {
            (
                ProviderKey::new(format!("m{at}")).unwrap(),
                "primary".to_owned(),
            )
        })
        .collect();
    let prompt = digest_prompt(7, &messages, &stored_categories);
    assert_eq!(
        prompt.matches("<untrusted heading=\"email\">").count(),
        5,
        "only the newest five of the category ride:\n{prompt}",
    );
    assert!(prompt.contains("Subject: Note 0"), "the newest rides");
    assert!(prompt.contains("Subject: Note 4"), "the fifth-newest rides");
    assert!(
        !prompt.contains("Subject: Note 5"),
        "the sixth-newest never rides:\n{prompt}",
    );
    assert!(
        prompt.contains("primary: 7"),
        "the rollup still counts every message, past the head cap:\n{prompt}",
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
        digest_prompt(0, &messages, &stored(&[])).contains("last 1 day"),
        "a zero-day ask clamps up to one",
    );
    assert!(
        digest_prompt(7, &messages, &stored(&[])).contains("last 7 days"),
        "an in-bounds ask rides unchanged",
    );
    assert!(
        digest_prompt(40, &messages, &stored(&[])).contains("last 31 days"),
        "an over-ceiling ask clamps down to thirty-one",
    );
}

#[test]
fn an_over_cap_body_carries_the_truncation_marker() {
    let mut long = message(0, false);
    long.body = format!("{}TAILMARK", "x".repeat(DOCUMENT_BODY_CAP + 50));
    let prompt = digest_prompt(7, std::slice::from_ref(&long), &stored(&[]));
    assert!(
        prompt.contains("[truncated]"),
        "an over-cap body carries the truncation marker:\n{prompt}",
    );
    assert!(
        !prompt.contains("TAILMARK"),
        "the text past the cap never rides:\n{prompt}",
    );
}

#[tokio::test]
async fn an_empty_window_makes_no_call_and_answers_empty() {
    let complete = FixtureComplete::scripted(&[]);
    let digest = generate_digest(
        &account(),
        7,
        &[],
        &stored(&[]),
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(digest, "");
    assert_eq!(complete.call_count(), 0, "no messages, no call");
}

#[tokio::test]
async fn a_failed_call_answers_the_typed_completion_error() {
    let complete = FixtureComplete::failing("llm unreachable: every attempt failed");
    let error = generate_digest(
        &account(),
        7,
        &[message(0, false)],
        &stored(&[]),
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap_err();
    match &error {
        DigestError::Completion(detail) => {
            assert_eq!(detail, "llm unreachable: every attempt failed");
        }
        DigestError::MalformedOutput(detail) => {
            panic!("the failure is the completion error, not a parse failure: {detail}");
        }
    }
}

#[tokio::test]
async fn unparseable_output_answers_the_typed_malformed_error() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let error = generate_digest(
        &account(),
        7,
        &[message(0, false)],
        &stored(&[]),
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, DigestError::MalformedOutput(_)),
        "the failure is the malformed-output error",
    );
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, the digest:\n{GOOD}\nHope that helps!");
    let complete = FixtureComplete::scripted(&[&answer]);
    let digest = generate_digest(
        &account(),
        7,
        &[message(0, false)],
        &stored(&[]),
        &CorrelationId::generate(),
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(digest, "Two notes need replies; the newsletter can wait.");
}
