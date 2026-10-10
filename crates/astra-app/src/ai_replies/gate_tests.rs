// SPDX-License-Identifier: GPL-3.0-only
//! Surfacing gate tests, over the fixture judge seam and an in-memory
//! pool store: the confidence floor and the judge's Yes (the gate clears
//! only on both), the abstain default hiding its chip, the top-two and
//! newest-first chips data, the trimmed one-line preview, the dismissal
//! marker consulted ahead of any judge call, and the seam pins (the
//! JudgeDraftSurface use case, the positive-pole question, the drafted
//! reply as the context half, the turn's correlation).

use astra_ai::{
    AiStore, CorrelationId, JudgeOutcome, JudgePath, JudgeUseCase, JudgeVerdict, NewDraftPoolRow,
};
use engine_api::{AccountId, ProviderKey};

use super::gate::{
    chips_for, surface_question, Chip, CHIP_CONFIDENCE_FLOOR, CHIP_LIMIT, CHIP_PREVIEW_CAP,
    GATE_JUDGE_VERSION,
};
use super::DraftConfidence;
use crate::dispatch_fakes::ai::FixtureJudge;

/// A first-try verdict.
fn first(verdict: JudgeVerdict) -> JudgeOutcome {
    JudgeOutcome {
        verdict,
        path: JudgePath::FirstTry,
    }
}

/// The abstain default for the draft-surfacing judge: verdict No, path
/// default, the shape the host's safe-default table answers.
fn abstain_default() -> JudgeOutcome {
    JudgeOutcome {
        verdict: JudgeVerdict::No,
        path: JudgePath::AbstainDefault,
    }
}

/// The gate's message identity halves.
fn identities() -> (AccountId, ProviderKey) {
    (
        AccountId::try_from("work").unwrap(),
        ProviderKey::new("m1").unwrap(),
    )
}

/// One drafted pool row landed for the gate's message.
fn drafted(pool: &AiStore, reply: &str, confidence: Option<f64>) {
    pool.insert_draft_pool(&NewDraftPoolRow {
        correlation: CorrelationId::generate(),
        message_account: "work".to_owned(),
        message_key: "m1".to_owned(),
        reply: Some(reply.to_owned()),
        confidence,
    })
    .expect("the pool row lands");
}

/// A confident yes judge: the gate's clearing shape.
fn yes_judge() -> FixtureJudge {
    FixtureJudge::scripted(&[first(JudgeVerdict::Yes)])
}

#[tokio::test]
async fn a_confident_yes_draft_surfaces_as_one_chip() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Yes, Friday is right. See you then.", Some(1.0));
    let judge = yes_judge();
    let correlation = CorrelationId::generate();
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &correlation).await;
    assert_eq!(
        chips,
        vec![Chip {
            reply: "Yes, Friday is right. See you then.".to_owned(),
            preview: "Yes, Friday is right. See you then.".to_owned(),
        }],
        "a high-confidence yes draft clears the gate",
    );
    assert_eq!(judge.call_count(), 1, "exactly one judge call");
    let call = &judge.calls()[0];
    assert_eq!(call.use_case, JudgeUseCase::JudgeDraftSurface);
    assert_eq!(call.correlation, correlation);
    assert_eq!(
        call.question,
        surface_question(),
        "the question half is the versioned one",
    );
    assert!(
        call.context.contains("Yes, Friday is right. See you then."),
        "the drafted reply rides the context half:\n{}",
        call.context,
    );
}

#[tokio::test]
async fn a_medium_confidence_draft_clears_the_floor() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "I will confirm by Monday.", Some(0.6));
    let judge = yes_judge();
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert_eq!(chips.len(), 1, "medium sits at the floor and clears it");
}

#[tokio::test]
async fn a_low_confidence_draft_never_reaches_the_judge() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Maybe Friday? I am not sure.", Some(0.3));
    let judge = FixtureJudge::scripted(&[]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(chips.is_empty(), "a low-confidence draft stays pooled");
    assert_eq!(
        judge.call_count(),
        0,
        "the floor filters before any judge call is spent",
    );
}

#[tokio::test]
async fn skipped_rows_never_reach_the_judge() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "", None);
    let judge = FixtureJudge::scripted(&[]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(chips.is_empty(), "a skipped row surfaces nothing");
    assert_eq!(judge.call_count(), 0);
}

#[tokio::test]
async fn a_blank_reply_never_reaches_the_judge() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "  \n \t", Some(1.0));
    let judge = FixtureJudge::scripted(&[]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(chips.is_empty(), "a blank draft is filler by definition");
    assert_eq!(judge.call_count(), 0);
}

#[tokio::test]
async fn a_no_verdict_hides_the_chip() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Thanks!", Some(1.0));
    let judge = FixtureJudge::scripted(&[first(JudgeVerdict::No)]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(
        chips.is_empty(),
        "the pool row stays queryable, the chip stays hidden",
    );
}

#[tokio::test]
async fn the_abstain_default_hides_the_chip() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Sure, sounds good.", Some(1.0));
    let judge = FixtureJudge::scripted(&[abstain_default()]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(chips.is_empty(), "fail quiet: the default is unsurfaced");
}

#[tokio::test]
async fn at_most_two_chips_surface_and_the_newest_win() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Oldest reply.", Some(1.0));
    drafted(&pool, "Middle reply.", Some(1.0));
    drafted(&pool, "Newest reply.", Some(1.0));
    drafted(&pool, "A skipped row between.", None);
    let judge = FixtureJudge::scripted(&[
        first(JudgeVerdict::Yes),
        first(JudgeVerdict::Yes),
        first(JudgeVerdict::Yes),
    ]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert_eq!(
        chips
            .iter()
            .map(|chip| chip.reply.as_str())
            .collect::<Vec<_>>(),
        vec!["Newest reply.", "Middle reply."],
        "the top two by creation order, newest first, the skipped row passed over",
    );
    assert_eq!(
        judge.call_count(),
        2,
        "the cap stops the spend once two chips stand; the oldest row is never judged",
    );
}

#[tokio::test]
async fn the_gate_falls_through_to_an_older_cleared_draft() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "The older, better reply.", Some(1.0));
    drafted(&pool, "The newer, weaker reply.", Some(0.6));
    let judge = FixtureJudge::scripted(&[first(JudgeVerdict::No), first(JudgeVerdict::Yes)]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert_eq!(
        chips
            .iter()
            .map(|chip| chip.reply.as_str())
            .collect::<Vec<_>>(),
        vec!["The older, better reply."],
        "a refused newest draft does not hide an older cleared one",
    );
}

#[tokio::test]
async fn a_dismissed_message_answers_no_chips_and_spends_no_call() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Yes, Friday is right. See you then.", Some(1.0));
    pool.dismiss_chips("work", "m1").expect("the marker lands");
    let judge = FixtureJudge::scripted(&[]);
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(chips.is_empty(), "the dismissal hides the message's chips");
    assert_eq!(
        judge.call_count(),
        0,
        "the marker is consulted ahead of any judge call",
    );
}

#[tokio::test]
async fn the_gate_records_no_ai_results_rows() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(&pool, "Yes, Friday is right. See you then.", Some(1.0));
    let judge = yes_judge();
    let (account, key) = identities();
    chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert!(
        pool.ai_results_for_message("work", "m1")
            .expect("reads")
            .is_empty(),
        "gate verdicts ride the host ledger only; a 0003 row would poison the pass's seen tracking",
    );
}

#[tokio::test]
async fn the_preview_is_trimmed_char_safe() {
    let pool = AiStore::open_in_memory().unwrap();
    let reply = "字".repeat(CHIP_PREVIEW_CAP + 20);
    drafted(&pool, &reply, Some(1.0));
    let judge = yes_judge();
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert_eq!(chips.len(), 1);
    assert_eq!(
        chips[0].preview,
        format!("{}...", "字".repeat(CHIP_PREVIEW_CAP)),
        "the cap counts characters and marks the cut",
    );
    assert_eq!(chips[0].reply, reply, "the chip keeps the whole reply");
}

#[tokio::test]
async fn the_preview_collapses_whitespace_to_one_line() {
    let pool = AiStore::open_in_memory().unwrap();
    drafted(
        &pool,
        "  Line one.\n\n   Line two,\ttabbed here.  ",
        Some(1.0),
    );
    let judge = yes_judge();
    let (account, key) = identities();
    let chips = chips_for(&account, &key, &pool, &judge, &CorrelationId::generate()).await;
    assert_eq!(chips[0].preview, "Line one. Line two, tabbed here.");
    assert_eq!(chips[0].reply, "Line one.\n\n   Line two,\ttabbed here.");
}

#[test]
fn the_gate_constants_are_pinned() {
    assert_eq!(GATE_JUDGE_VERSION, "1", "the pipeline constant is pinned");
    assert_eq!(CHIP_LIMIT, 2, "the reading pane shows at most two");
    assert_eq!(CHIP_PREVIEW_CAP, 80, "the preview budget");
    assert!(
        (CHIP_CONFIDENCE_FLOOR - DraftConfidence::Medium.score()).abs() < f64::EPSILON
            && (CHIP_CONFIDENCE_FLOOR - DraftConfidence::High.score()) < 0.0,
        "the floor sits at medium: medium and high clear, low does not",
    );
    assert_eq!(
        serde_json::to_string(&Chip {
            reply: "Yes.".to_owned(),
            preview: "Yes.".to_owned(),
        })
        .unwrap(),
        r#"{"reply":"Yes.","preview":"Yes."}"#,
        "the chip crosses the wire in its two-field shape",
    );
}
