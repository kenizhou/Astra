// SPDX-License-Identifier: GPL-3.0-only
//! Cached summarise tests, over the fixture completion seam and an
//! in-memory cache store: the miss path (the Summarise use case under the
//! turn's correlation, the structured shape, the repair path), the hit
//! path (the cached text with no seam call), the hash invalidation on a
//! changed body, the failures that cache nothing, and the prompt pins
//! (the untrusted wrap, the capped body, the output policy).

use astra_ai::{AiStore, CorrelationId, LlmUseCase};
use engine_api::{AccountId, ProviderKey};

use super::summarise::{
    body_hash, summarise, summarise_prompt, SummariseError, SUMMARISE_PROMPT_VERSION,
};
use crate::ai_rules::{MailContext, MATCH_CONTEXT_BODY_CAP};
use crate::dispatch_fakes::ai::FixtureComplete;

/// One mail context: Ada's launch note, the body the caller names.
fn mail(body: &str) -> MailContext {
    MailContext {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("m1").unwrap(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        subject: "Launch".to_owned(),
        body: body.to_owned(),
    }
}

/// The good fixture: one digest-shaped summary.
const GOOD: &str = r#"{"summary":"Ada asks for the launch date; the reply confirms Friday."}"#;

#[tokio::test]
async fn a_fresh_summary_runs_the_use_case_and_caches() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let cache = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let summary = summarise(
        &mail("Can you confirm the launch is Friday?"),
        &correlation,
        &complete,
        &cache,
    )
    .await
    .expect("the fresh summary parses");
    assert_eq!(
        summary,
        "Ada asks for the launch date; the reply confirms Friday."
    );
    assert_eq!(complete.call_count(), 1);
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::Summarise);
    assert_eq!(call.correlation, correlation);
}

#[tokio::test]
async fn a_cached_call_returns_without_a_seam_call() {
    let complete = FixtureComplete::scripted(&[GOOD, GOOD]);
    let cache = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let mail = mail("Can you confirm the launch is Friday?");
    let first = summarise(&mail, &correlation, &complete, &cache)
        .await
        .expect("the fresh summary parses");
    let second = summarise(&mail, &correlation, &complete, &cache)
        .await
        .expect("the cached summary answers");
    assert_eq!(first, second, "the hit returns the cached text");
    assert_eq!(
        complete.call_count(),
        1,
        "the cache hit spends no second call",
    );
}

#[tokio::test]
async fn a_changed_body_summarises_fresh_and_replaces_the_entry() {
    let complete = FixtureComplete::scripted(&[
        GOOD,
        r#"{"summary":"The body changed; this is the fresh summary."}"#,
    ]);
    let cache = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let original = mail("Can you confirm the launch is Friday?");
    summarise(&original, &correlation, &complete, &cache)
        .await
        .expect("the first summary parses");
    let changed = mail("The launch moved; can you confirm Monday instead?");
    let second = summarise(&changed, &correlation, &complete, &cache)
        .await
        .expect("the changed body summarises fresh");
    assert_eq!(second, "The body changed; this is the fresh summary.");
    assert_eq!(complete.call_count(), 2);
    let third = summarise(&changed, &correlation, &complete, &cache)
        .await
        .expect("the new body's entry now hits");
    assert_eq!(third, second, "the re-put replaced the old entry");
    assert_eq!(
        complete.call_count(),
        2,
        "the replaced entry serves the second read of the changed body",
    );
}

#[tokio::test]
async fn a_failed_call_errors_and_caches_nothing() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let cache = AiStore::open_in_memory().unwrap();
    let mail = mail("Can you confirm the launch is Friday?");
    let error = summarise(&mail, &CorrelationId::generate(), &complete, &cache)
        .await
        .expect_err("the seam failure propagates");
    assert!(
        matches!(&error, SummariseError::Completion(message) if message.contains("llm unreachable")),
        "the host's failure rides the error: {error}",
    );
    let recover = FixtureComplete::scripted(&[GOOD]);
    let summary = summarise(&mail, &CorrelationId::generate(), &recover, &cache)
        .await
        .expect("the retry runs fresh");
    assert_eq!(recover.call_count(), 1, "nothing was cached on the failure");
    assert_eq!(
        summary,
        "Ada asks for the launch date; the reply confirms Friday."
    );
}

#[tokio::test]
async fn an_unparseable_output_errors_and_caches_nothing() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let cache = AiStore::open_in_memory().unwrap();
    let mail = mail("Can you confirm the launch is Friday?");
    let error = summarise(&mail, &CorrelationId::generate(), &complete, &cache)
        .await
        .expect_err("the garbage output propagates");
    assert!(
        matches!(error, SummariseError::MalformedOutput(_)),
        "the parse failure names itself: {error}",
    );
    let recover = FixtureComplete::scripted(&[GOOD]);
    summarise(&mail, &CorrelationId::generate(), &recover, &cache)
        .await
        .expect("the retry runs fresh");
    assert_eq!(recover.call_count(), 1, "nothing was cached on the garbage");
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, the summary:\n{GOOD}\nHope that helps!");
    let complete = FixtureComplete::scripted(&[&answer]);
    let cache = AiStore::open_in_memory().unwrap();
    let summary = summarise(
        &mail("Can you confirm the launch is Friday?"),
        &CorrelationId::generate(),
        &complete,
        &cache,
    )
    .await
    .expect("the repaired completion summarises");
    assert_eq!(
        summary,
        "Ada asks for the launch date; the reply confirms Friday."
    );
}

#[test]
fn the_body_hash_is_stable_and_body_sensitive() {
    let a = body_hash("Can you confirm the launch is Friday?");
    assert_eq!(a, body_hash("Can you confirm the launch is Friday?"));
    assert_ne!(
        a,
        body_hash("The launch moved; can you confirm Monday instead?")
    );
    assert_eq!(a.len(), 16, "the hash renders as sixteen hex digits: {a}");
    assert!(
        a.chars().all(|character| character.is_ascii_hexdigit()),
        "the hash renders as hex: {a}",
    );
}

#[test]
fn the_prompt_pins_its_halves_and_their_cap() {
    let short = mail("Can you confirm the launch is Friday?");
    let prompt = summarise_prompt(&short);
    assert!(
        prompt.contains("<untrusted heading=\"email\">"),
        "the mail context rides the hardening wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("From: Ada Lovelace"),
        "the shared one-message context builds the wrap's content:\n{prompt}",
    );
    assert!(
        prompt.contains(r#"{"summary":"..."}"#),
        "the schema example rides:\n{prompt}",
    );
    assert_eq!(SUMMARISE_PROMPT_VERSION, "1");
    let long = mail(&format!(
        "{}TAILMARK",
        "x".repeat(MATCH_CONTEXT_BODY_CAP + 50)
    ));
    let prompt = summarise_prompt(&long);
    assert!(
        prompt.contains("[truncated]"),
        "an over-cap body carries the truncation marker:\n{prompt}",
    );
    assert!(
        !prompt.contains("TAILMARK"),
        "the text past the cap never rides:\n{prompt}",
    );
}
