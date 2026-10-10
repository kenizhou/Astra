// SPDX-License-Identifier: GPL-3.0-only
//! Cold stage tests, over the fixture judge seam and an in-memory results
//! store: the three verdict outcomes (yes is cold, no is not, the abstain
//! default is not cold and is recorded as the default it was), the seam
//! pins (the use case, the positive-pole question, the shared capped
//! context, the correlation), and the pipeline constant.

use astra_ai::{AiStore, CorrelationId, JudgeOutcome, JudgePath, JudgeUseCase, JudgeVerdict};
use engine_api::{AccountId, ProviderKey};

use super::cold::{cold_question, judge_cold, COLD_JUDGE_VERSION};
use super::matching::{judge_context, MailContext, MATCH_CONTEXT_BODY_CAP};
use crate::dispatch_fakes::ai::FixtureJudge;

/// A first-try verdict.
fn first(verdict: JudgeVerdict) -> JudgeOutcome {
    JudgeOutcome {
        verdict,
        path: JudgePath::FirstTry,
    }
}

/// The abstain default for the cold judge: verdict No (not cold), path
/// default.
fn abstain_default() -> JudgeOutcome {
    JudgeOutcome {
        verdict: JudgeVerdict::No,
        path: JudgePath::AbstainDefault,
    }
}

/// One mail context: a stranger's pitch.
fn mail(body: &str) -> MailContext {
    MailContext {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("m1").unwrap(),
        from_name: "Stranger".to_owned(),
        from_address: "stranger@elsewhere.test".to_owned(),
        subject: "Quick question".to_owned(),
        body: body.to_owned(),
    }
}

#[tokio::test]
async fn a_yes_verdict_is_cold_and_records_the_row() {
    let judge = FixtureJudge::scripted(&[first(JudgeVerdict::Yes)]);
    let store = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let outcome = judge_cold(
        &mail("I help startups like yours scale."),
        &correlation,
        &judge,
        &store,
    )
    .await;
    assert_eq!(outcome.verdict, JudgeVerdict::Yes);
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(rows.len(), 1, "the stage landed one 0003 row");
    assert_eq!(rows[0].stage, "cold");
    assert_eq!(rows[0].verdict, "yes");
    assert_eq!(rows[0].rule_id, None);
    assert_eq!(rows[0].category, None);
    assert_eq!(
        rows[0].correlation, correlation,
        "the pass id rides the row"
    );
}

#[tokio::test]
async fn a_no_verdict_is_not_cold_and_still_records() {
    let judge = FixtureJudge::scripted(&[first(JudgeVerdict::No)]);
    let store = AiStore::open_in_memory().unwrap();
    let outcome = judge_cold(
        &mail("Thanks for the review, see you Monday."),
        &CorrelationId::generate(),
        &judge,
        &store,
    )
    .await;
    assert_eq!(outcome.verdict, JudgeVerdict::No);
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].verdict, "no", "the no is recorded, not dropped");
}

#[tokio::test]
async fn the_abstain_default_is_not_cold_and_is_recorded_as_the_default() {
    let judge = FixtureJudge::scripted(&[abstain_default()]);
    let store = AiStore::open_in_memory().unwrap();
    let outcome = judge_cold(
        &mail("An unjudgeable mess."),
        &CorrelationId::generate(),
        &judge,
        &store,
    )
    .await;
    assert_eq!(
        outcome.verdict,
        JudgeVerdict::No,
        "the table's answer is not cold, so the mail stays put"
    );
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(
        rows[0].verdict, "abstain_default",
        "the row names the table's answer"
    );
}

#[tokio::test]
async fn the_seam_receives_the_cold_use_case_question_and_capped_context() {
    let judge = FixtureJudge::scripted(&[first(JudgeVerdict::No)]);
    let store = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let long_body = "q".repeat(MATCH_CONTEXT_BODY_CAP + 500);
    judge_cold(&mail(&long_body), &correlation, &judge, &store).await;
    assert_eq!(judge.call_count(), 1, "exactly one judge call");
    let call = &judge.calls()[0];
    assert_eq!(call.use_case, JudgeUseCase::JudgeColdEmail);
    assert_eq!(call.correlation, correlation);
    assert_eq!(
        call.question,
        cold_question(),
        "the question is the pinned one"
    );
    assert_eq!(
        call.context,
        judge_context(&mail(&long_body)),
        "the context is the shared capped builder, the same shape the match rides",
    );
    assert!(
        call.context.contains("[truncated]"),
        "an over-cap body names its truncation",
    );
}

#[test]
fn the_question_pins_yes_as_the_cold_pole() {
    let question = cold_question();
    assert!(
        question.trim_end().ends_with('?'),
        "a question, phrased for a one-bit answer:\n{question}",
    );
    assert!(
        question.contains("cold"),
        "the positive pole is the cold verdict:\n{question}",
    );
    assert_eq!(COLD_JUDGE_VERSION, "1", "the pipeline constant is pinned");
}
