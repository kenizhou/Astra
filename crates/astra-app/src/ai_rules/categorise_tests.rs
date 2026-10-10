// SPDX-License-Identifier: GPL-3.0-only
//! Categorisation tests, over the fixture completion seam and an in-memory
//! results store: the five-way closed set, the good parse and the repair
//! path, the unknown word and garbage completions that fail loudly and
//! record the stage as skipped, the failed seam call, and the prompt pins
//! (the use case, the correlation, the versioned instruction, the JSON
//! output policy, and the mail riding inside the untrusted wrap).

use astra_ai::{AiStore, CorrelationId, LlmUseCase};
use engine_api::{AccountId, ProviderKey};

use super::categorise::{
    categorise, categorise_prompt, CategoriseError, Category, CATEGORISE_PROMPT_VERSION,
};
use super::matching::MailContext;
use crate::dispatch_fakes::ai::FixtureComplete;

/// One mail context: Ada's newsletter-shaped message.
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

#[tokio::test]
async fn a_good_completion_categorises_and_records_the_row() {
    let complete = FixtureComplete::scripted(&[r#"{"category":"newsletter"}"#]);
    let store = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let category = categorise(&mail("Our weekly digest."), &correlation, &complete, &store)
        .await
        .expect("a good completion categorises");
    assert_eq!(category, Category::Newsletter);
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(rows.len(), 1, "the stage landed one 0003 row");
    assert_eq!(rows[0].stage, "categorise");
    assert_eq!(rows[0].verdict, "ok");
    assert_eq!(rows[0].category.as_deref(), Some("newsletter"));
    assert_eq!(rows[0].rule_id, None, "a message-level stage names no rule");
    assert_eq!(
        rows[0].correlation, correlation,
        "the pass id rides the row"
    );
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = r#"Sure: {"category":"receipt"} hope that helps"#;
    let complete = FixtureComplete::scripted(&[answer]);
    let store = AiStore::open_in_memory().unwrap();
    let category = categorise(
        &mail("Your order shipped."),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await
    .expect("the repaired completion parses");
    assert_eq!(category, Category::Receipt);
    assert_eq!(
        store
            .ai_results_for_message("work", "m1")
            .expect("reads")
            .len(),
        1,
    );
}

#[tokio::test]
async fn an_unknown_category_word_fails_loudly_and_records_the_skip() {
    let complete = FixtureComplete::scripted(&[r#"{"category":"marketing"}"#]);
    let store = AiStore::open_in_memory().unwrap();
    let err = categorise(
        &mail("Bulk marketing."),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await
    .expect_err("a word outside the closed set must fail, not guess");
    assert!(matches!(err, CategoriseError::MalformedOutput(_)), "{err}");
    assert!(err.to_string().contains("marketing"), "{err}");
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(rows.len(), 1, "the failed stage still lands its row");
    assert_eq!(rows[0].stage, "categorise");
    assert_eq!(rows[0].verdict, "skipped");
    assert_eq!(rows[0].category, None);
}

#[tokio::test]
async fn garbage_completions_fail_loudly_and_skip() {
    for garbage in ["no json here at all", "", "{\"category\":", "[]"] {
        let complete = FixtureComplete::scripted(&[garbage]);
        let store = AiStore::open_in_memory().unwrap();
        let err = categorise(
            &mail("Anything."),
            &CorrelationId::generate(),
            &complete,
            &store,
        )
        .await
        .expect_err("an unparseable completion must fail");
        assert!(
            matches!(err, CategoriseError::MalformedOutput(_)),
            "{garbage:?}: {err}",
        );
        let rows = store.ai_results_for_message("work", "m1").expect("reads");
        assert_eq!(
            (rows[0].stage.as_str(), rows[0].verdict.as_str()),
            ("categorise", "skipped"),
            "{garbage:?}: the skip is recorded",
        );
    }
}

#[tokio::test]
async fn a_failed_completion_propagates_the_seam_error_and_skips() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let store = AiStore::open_in_memory().unwrap();
    let err = categorise(
        &mail("Anything."),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await
    .expect_err("a seam failure must surface, not default");
    assert!(matches!(err, CategoriseError::Completion(_)), "{err}");
    assert!(err.to_string().contains("llm unreachable"), "{err}");
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(
        (rows[0].stage.as_str(), rows[0].verdict.as_str()),
        ("categorise", "skipped")
    );
}

#[tokio::test]
async fn the_seam_receives_the_versioned_prompt_over_the_wrapped_context() {
    let complete = FixtureComplete::scripted(&[r#"{"category":"primary"}"#]);
    let store = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    categorise(
        &mail("Weekly notes from Ada."),
        &correlation,
        &complete,
        &store,
    )
    .await
    .expect("categorises");
    assert_eq!(complete.call_count(), 1, "exactly one completion call");
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::CategorizeSender);
    assert_eq!(call.correlation, correlation);
    let prompt = &call.prompt;
    for word in ["primary", "newsletter", "receipt", "notification", "cold"] {
        assert!(
            prompt.contains(word),
            "the closed set teaches {word}:\n{prompt}"
        );
    }
    assert!(
        prompt.contains("Return a JSON object only"),
        "the output policy rides:\n{prompt}",
    );
    assert!(
        prompt.contains(r#"{"category":"primary"}"#),
        "the schema example rides:\n{prompt}",
    );
    // The mail content rides inside the untrusted wrap and nowhere else.
    let begin = prompt
        .find("[[[untrusted:begin]]]")
        .expect("the wrap opens");
    let end = prompt.find("[[[untrusted:end]]]").expect("the wrap closes");
    let from = prompt
        .find("From: Ada Lovelace <ada@example.test>")
        .expect("the sender rides the context");
    assert!(
        begin < from && from < end,
        "the mail sits between the fences:\n{prompt}",
    );
}

#[test]
fn the_closed_category_set_is_pinned() {
    let words: Vec<&str> = Category::ALL.iter().map(Category::as_str).collect();
    assert_eq!(
        words,
        vec!["primary", "newsletter", "receipt", "notification", "cold"],
    );
    assert_eq!(
        serde_json::to_string(&Category::ALL).unwrap(),
        r#"["primary","newsletter","receipt","notification","cold"]"#,
    );
    let parsed: Category = serde_json::from_str("\"receipt\"").expect("the wire word parses");
    assert_eq!(parsed, Category::Receipt);
    assert!(
        serde_json::from_str::<Category>("\"marketing\"").is_err(),
        "an unknown wire word is rejected at parse",
    );
    assert_eq!(
        CATEGORISE_PROMPT_VERSION, "1",
        "the pipeline constant is pinned"
    );
}

#[test]
fn the_prompt_builder_wraps_the_context_it_is_given() {
    let prompt = categorise_prompt("From: a@example.test\nSubject: S\n\nBody.");
    assert!(prompt.contains("[[[untrusted:begin]]]"));
    assert!(
        prompt.ends_with("</untrusted>"),
        "the wrap is the prompt's tail"
    );
}
