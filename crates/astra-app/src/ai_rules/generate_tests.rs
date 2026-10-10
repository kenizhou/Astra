// SPDX-License-Identifier: GPL-3.0-only
//! AI rules tests: the closed action set and its wire words, the store row
//! bridge (unknown actions rejected loudly), and the generation shape over
//! a fixture seam: good JSON, prose around the JSON (the repair path),
//! garbage, unknown actions, and a failed completion. The prompt, the use
//! case, and the correlation the seam receives are pinned, and the
//! pipeline constant rides the prompt.

use std::sync::Mutex;

use astra_ai::{repair_json, AiStore, CorrelationId, LlmUseCase, RuleId, RuleRow};
use async_trait::async_trait;

use super::{prompt_to_rules, Action, AiComplete, Rule, RulesError, RULES_PROMPT_VERSION};

/// One call the fixture seam received, frozen for the assertions.
struct Call {
    use_case: LlmUseCase,
    prompt: String,
    correlation: CorrelationId,
}

/// A completion seam that answers from a canned script and records every
/// call: the tests pin what generation sends by reading it back.
struct FixtureSeam {
    answers: Mutex<Vec<Result<String, String>>>,
    calls: Mutex<Vec<Call>>,
}

impl FixtureSeam {
    /// A seam that answers the given strings in order, every one a success.
    fn scripted(answers: &[&str]) -> Self {
        Self {
            answers: Mutex::new(
                answers
                    .iter()
                    .map(|answer| Ok((*answer).to_string()))
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// A seam that always fails with `message`.
    fn failing(message: &str) -> Self {
        Self {
            answers: Mutex::new(vec![Err(message.to_string())]),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// The one call the seam received; panics on any other count.
    fn the_call(&self) -> Call {
        let calls = self.calls.lock().expect("fixture calls unpoisoned");
        assert_eq!(calls.len(), 1, "exactly one completion call");
        Call {
            use_case: calls[0].use_case,
            prompt: calls[0].prompt.clone(),
            correlation: calls[0].correlation.clone(),
        }
    }
}

#[async_trait]
impl AiComplete for FixtureSeam {
    async fn complete(
        &self,
        use_case: LlmUseCase,
        prompt: &str,
        correlation: &CorrelationId,
    ) -> Result<String, String> {
        self.calls
            .lock()
            .expect("fixture calls unpoisoned")
            .push(Call {
                use_case,
                prompt: prompt.to_string(),
                correlation: correlation.clone(),
            });
        self.answers
            .lock()
            .expect("fixture answers unpoisoned")
            .remove(0)
    }
}

/// The good fixture: two rules, both actions of the closed set in play.
const GOOD_JSON: &str = r#"{"rules":[
    {"name":"Newsletters","condition":"The message is a newsletter","actions":["mark_read"]},
    {"name":"Flag the boss","condition":"The sender is my manager","actions":["set_flagged","mark_read"]}
]}"#;

#[tokio::test]
async fn good_json_generates_enabled_rules_with_fresh_ids() {
    let seam = FixtureSeam::scripted(&[GOOD_JSON]);
    let rules = prompt_to_rules(
        "Mark newsletters read and flag my manager",
        &CorrelationId::generate(),
        &seam,
    )
    .await
    .expect("good JSON generates");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].name, "Newsletters");
    assert_eq!(rules[0].condition, "The message is a newsletter");
    assert_eq!(rules[0].actions, vec![Action::MarkRead]);
    assert_eq!(rules[1].actions, vec![Action::SetFlagged, Action::MarkRead]);
    for rule in &rules {
        assert!(rule.enabled, "a generated rule starts enabled");
        assert!(!rule.id.as_str().is_empty(), "the id is minted, not blank");
    }
    assert_ne!(rules[0].id, rules[1].id, "each rule gets its own id");
}

#[tokio::test]
async fn the_seam_receives_the_versioned_prompt_and_the_turn_correlation() {
    let seam = FixtureSeam::scripted(&[GOOD_JSON]);
    let correlation = CorrelationId::generate();
    prompt_to_rules("Flag my manager", &correlation, &seam)
        .await
        .expect("generates");
    let call = seam.the_call();
    assert_eq!(call.use_case, LlmUseCase::PromptToRules);
    assert_eq!(call.correlation, correlation);
    let prompt = call.prompt;
    assert!(prompt.contains("mark_read"), "{prompt}");
    assert!(prompt.contains("set_flagged"), "{prompt}");
    assert!(prompt.contains("JSON object only"), "{prompt}");
    assert!(
        prompt.contains("<prompt>\nFlag my manager\n</prompt>"),
        "the user text rides wrapped and intact:\n{prompt}",
    );
    assert_eq!(RULES_PROMPT_VERSION, "1", "the pipeline constant is pinned");
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, here are your rules:\n{GOOD_JSON}\nHope that helps!");
    let seam = FixtureSeam::scripted(&[&answer]);
    let rules = prompt_to_rules("newsletters", &CorrelationId::generate(), &seam)
        .await
        .expect("the repaired completion parses");
    assert_eq!(rules.len(), 2);
}

#[tokio::test]
async fn markdown_fenced_json_parses() {
    let fenced = format!("```json\n{GOOD_JSON}\n```");
    let seam = FixtureSeam::scripted(&[&fenced]);
    let rules = prompt_to_rules("newsletters", &CorrelationId::generate(), &seam)
        .await
        .expect("a fenced object is prose around the JSON");
    assert_eq!(rules.len(), 2);
}

#[tokio::test]
async fn garbage_completions_fail_loudly() {
    for garbage in ["no json here at all", "", "{\"rules\": [{\"name\":", "[]"] {
        let seam = FixtureSeam::scripted(&[garbage]);
        let err = prompt_to_rules("anything", &CorrelationId::generate(), &seam)
            .await
            .expect_err("an unparseable completion must fail, not guess");
        assert!(
            matches!(err, RulesError::MalformedOutput(_)),
            "{garbage:?}: {err}",
        );
    }
}

#[tokio::test]
async fn an_empty_rules_array_is_an_honest_empty_result() {
    let seam = FixtureSeam::scripted(&[r#"{"rules":[]}"#]);
    let rules = prompt_to_rules("nothing applicable", &CorrelationId::generate(), &seam)
        .await
        .expect("an empty array parses");
    assert!(rules.is_empty());
}

#[tokio::test]
async fn an_unknown_action_is_rejected_with_the_word_named() {
    let seam = FixtureSeam::scripted(&[
        r#"{"rules":[{"name":"X","condition":"y","actions":["archive"]}]}"#,
    ]);
    let err = prompt_to_rules("archive everything", &CorrelationId::generate(), &seam)
        .await
        .expect_err("an action outside the closed set must fail the generation");
    let message = err.to_string();
    assert!(message.contains("archive"), "{message}");
    assert!(message.contains("mark_read"), "{message}");
}

#[tokio::test]
async fn a_failed_completion_propagates_the_seam_error() {
    let seam = FixtureSeam::failing("llm unreachable: all entries failed");
    let err = prompt_to_rules("anything", &CorrelationId::generate(), &seam)
        .await
        .expect_err("a seam failure must surface, not default");
    assert!(matches!(err, RulesError::Completion(_)), "{err}");
    assert!(err.to_string().contains("llm unreachable"), "{err}");
}

#[test]
fn the_closed_action_set_is_pinned() {
    let words: Vec<&str> = Action::ALL.iter().map(Action::as_str).collect();
    assert_eq!(words, vec!["mark_read", "set_flagged"]);
    assert_eq!(
        serde_json::to_string(&Action::ALL).unwrap(),
        r#"["mark_read","set_flagged"]"#,
    );
    let parsed: Vec<Action> =
        serde_json::from_str(r#"["set_flagged","mark_read"]"#).expect("both wire words parse");
    assert_eq!(parsed, vec![Action::SetFlagged, Action::MarkRead]);
    let err = serde_json::from_str::<Vec<Action>>(r#"["star"]"#)
        .expect_err("an unknown wire word is rejected at parse");
    let message = err.to_string();
    assert!(message.contains("star"), "{message}");
    assert!(message.contains("mark_read"), "{message}");
}

#[test]
fn rules_round_trip_through_the_store_row() {
    let rule = Rule {
        id: RuleId::generate(),
        name: "Newsletters".to_string(),
        condition: "The message is a newsletter".to_string(),
        actions: vec![Action::MarkRead, Action::SetFlagged],
        enabled: false,
    };
    let store = AiStore::open_in_memory().expect("the store opens");
    store
        .create_rule(&rule.as_new_rule())
        .expect("the rule stores");
    let rows = store.list_rules().expect("the rows list");
    assert_eq!(rows.len(), 1);
    let loaded =
        Rule::try_from(rows.into_iter().next().expect("the one row")).expect("the row parses back");
    assert_eq!(loaded, rule, "every field survives the store round trip");
}

/// A minimal stored row, for the payload cases.
fn row_for(actions: &str) -> RuleRow {
    RuleRow {
        id: RuleId::new("rule-1".to_string()),
        name: "Broken".to_string(),
        condition: "anything".to_string(),
        actions: actions.to_string(),
        enabled: true,
        created_at: 1000,
    }
}

#[test]
fn a_stored_row_with_an_unknown_action_is_rejected_loudly() {
    let err = Rule::try_from(row_for(r#"["archive"]"#))
        .expect_err("an action outside the closed set must fail the load");
    assert!(matches!(err, RulesError::UnknownAction { .. }), "{err}");
    let message = err.to_string();
    assert!(message.contains("archive"), "{message}");
    assert!(message.contains("mark_read"), "{message}");
}

#[test]
fn a_stored_row_with_malformed_actions_json_is_rejected_loudly() {
    let err = Rule::try_from(row_for("not json"))
        .expect_err("a malformed actions payload must fail the load");
    assert!(matches!(err, RulesError::UnknownAction { .. }), "{err}");
}

#[test]
fn the_repair_helper_is_the_shared_one() {
    // The generation path repairs through the AI foundation's helper, the
    // one the desktop host re-exports: not a copy of it.
    assert_eq!(repair_json("prose {\"a\":1} tail"), Some("{\"a\":1}"));
}
