// SPDX-License-Identifier: GPL-3.0-only
//! Translate tests, over the fixture completion seam and an in-memory
//! text cache: the fresh path (the Translate use case under the turn's
//! correlation, the structured shape, the repair path, the cache put),
//! the hit path (the cached translation with no seam call), the
//! invalidation (another language or a changed body derives fresh), the
//! failures that cache nothing, the empty body that never calls, and the
//! pins (the language set with its default, the cache kind, the prompt's
//! named language and untrusted wrap, the version).

use astra_ai::{AiStore, CorrelationId, LlmUseCase};
use engine_api::{AccountId, ProviderKey};

use super::translate::{
    translate, translate_prompt, TranslateLanguage, TRANSLATE_CACHE_KIND, TRANSLATE_VERSION,
};
use crate::dispatch_fakes::ai::FixtureComplete;

/// The account owning the open message.
fn account() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// The open message's provider key.
fn key() -> ProviderKey {
    ProviderKey::new("m1").unwrap()
}

/// The good fixture: one German translation of the launch sentence.
const GOOD: &str = r#"{"translation":"Der Start ist am Freitag."}"#;

#[tokio::test]
async fn a_fresh_translation_runs_the_use_case_and_caches() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let cache = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let first = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &correlation,
        &complete,
        &cache,
    )
    .await;
    assert_eq!(first, "Der Start ist am Freitag.");
    assert_eq!(complete.call_count(), 1);
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::Translate);
    assert_eq!(call.correlation, correlation);
    let second = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &correlation,
        &complete,
        &cache,
    )
    .await;
    assert_eq!(second, first, "the hit returns the cached translation");
    assert_eq!(
        complete.call_count(),
        1,
        "the cache hit spends no second call",
    );
}

#[tokio::test]
async fn another_language_or_a_changed_body_derives_fresh() {
    let complete = FixtureComplete::scripted(&[
        GOOD,
        r#"{"translation":"Le lancement est vendredi."}"#,
        r#"{"translation":"Der Start ist am Montag."}"#,
        r#"{"translation":"Der Start ist am Freitag."}"#,
    ]);
    let cache = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let friday_german = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &correlation,
        &complete,
        &cache,
    )
    .await;
    assert_eq!(friday_german, "Der Start ist am Freitag.");
    let french = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::French,
        &correlation,
        &complete,
        &cache,
    )
    .await;
    assert_eq!(
        french, "Le lancement est vendredi.",
        "the language keys the cache apart",
    );
    let monday_german = translate(
        &account(),
        &key(),
        "The launch is on Monday.",
        TranslateLanguage::German,
        &correlation,
        &complete,
        &cache,
    )
    .await;
    assert_eq!(
        monday_german, "Der Start ist am Montag.",
        "the body hash keys the cache apart",
    );
    assert_eq!(complete.call_count(), 3, "each key derives exactly once");
    assert_eq!(
        translate(
            &account(),
            &key(),
            "The launch is on Monday.",
            TranslateLanguage::German,
            &correlation,
            &complete,
            &cache,
        )
        .await,
        monday_german,
    );
    assert_eq!(
        translate(
            &account(),
            &key(),
            "The launch is on Friday.",
            TranslateLanguage::French,
            &correlation,
            &complete,
            &cache,
        )
        .await,
        french,
        "the french row of the same body still hits",
    );
    assert_eq!(complete.call_count(), 3, "both live keys hit");
    assert_eq!(
        translate(
            &account(),
            &key(),
            "The launch is on Friday.",
            TranslateLanguage::German,
            &correlation,
            &complete,
            &cache,
        )
        .await,
        friday_german,
        "the german row the Monday put replaced derives fresh",
    );
    assert_eq!(complete.call_count(), 4, "the replaced row is a miss");
    assert_eq!(
        translate(
            &account(),
            &key(),
            "The launch is on Friday.",
            TranslateLanguage::German,
            &correlation,
            &complete,
            &cache,
        )
        .await,
        friday_german,
        "the re-derived german row now hits",
    );
    assert_eq!(complete.call_count(), 4);
}

#[tokio::test]
async fn a_failed_call_answers_empty_and_caches_nothing() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let cache = AiStore::open_in_memory().unwrap();
    let answered = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &CorrelationId::generate(),
        &complete,
        &cache,
    )
    .await;
    assert_eq!(answered, "", "the failure answers nothing, no crash");
    let recover = FixtureComplete::scripted(&[GOOD]);
    let retried = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &CorrelationId::generate(),
        &recover,
        &cache,
    )
    .await;
    assert_eq!(retried, "Der Start ist am Freitag.");
    assert_eq!(recover.call_count(), 1, "nothing was cached on the failure");
}

#[tokio::test]
async fn an_unparseable_output_answers_empty_and_caches_nothing() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let cache = AiStore::open_in_memory().unwrap();
    let answered = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &CorrelationId::generate(),
        &complete,
        &cache,
    )
    .await;
    assert_eq!(answered, "");
    let recover = FixtureComplete::scripted(&[GOOD]);
    translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &CorrelationId::generate(),
        &recover,
        &cache,
    )
    .await;
    assert_eq!(recover.call_count(), 1, "nothing was cached on the garbage");
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, the translation:\n{GOOD}\nHope that helps!");
    let complete = FixtureComplete::scripted(&[&answer]);
    let cache = AiStore::open_in_memory().unwrap();
    let answered = translate(
        &account(),
        &key(),
        "The launch is on Friday.",
        TranslateLanguage::German,
        &CorrelationId::generate(),
        &complete,
        &cache,
    )
    .await;
    assert_eq!(answered, "Der Start ist am Freitag.");
}

#[tokio::test]
async fn an_empty_body_answers_empty_without_a_call() {
    let complete = FixtureComplete::scripted(&[]);
    let cache = AiStore::open_in_memory().unwrap();
    let answered = translate(
        &account(),
        &key(),
        "   ",
        TranslateLanguage::English,
        &CorrelationId::generate(),
        &complete,
        &cache,
    )
    .await;
    assert_eq!(answered, "");
    assert_eq!(complete.call_count(), 0, "nothing to translate, no call");
}

#[test]
fn the_language_set_is_pinned_with_its_default() {
    let words: Vec<&str> = TranslateLanguage::ALL
        .iter()
        .map(TranslateLanguage::as_str)
        .collect();
    assert_eq!(
        words,
        vec!["english", "chinese", "german", "french", "japanese", "spanish"],
        "the fixed list is the spec's six",
    );
    assert_eq!(
        TranslateLanguage::default(),
        TranslateLanguage::English,
        "English is the default target",
    );
    assert_eq!(TranslateLanguage::German.label(), "German");
    assert_eq!(TRANSLATE_CACHE_KIND, "translate");
}

#[test]
fn the_prompt_names_the_language_and_wraps_the_body() {
    let prompt = translate_prompt("The launch is on Friday.", TranslateLanguage::German);
    assert!(
        prompt.contains("Target language: German"),
        "the target language is named:\n{prompt}",
    );
    assert!(
        prompt.contains("<untrusted heading=\"email body\">"),
        "the body rides the untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("The launch is on Friday."),
        "the body rides whole:\n{prompt}",
    );
    assert!(
        prompt.contains(r#"{"translation":"..."}"#),
        "the schema example rides:\n{prompt}",
    );
    assert_eq!(TRANSLATE_VERSION, "1");
}
