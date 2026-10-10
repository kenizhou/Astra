// SPDX-License-Identifier: GPL-3.0-only
//! The pass's stage-order and dlp-gate tests, a child of the incoming
//! pass test module (it owns the fixtures these share). The gate runs in
//! front of every stage prompt: a blocked message spends no seam call and
//! lands one honestly blocked row, and a redacting policy masks every
//! prompt's content; the stages run in the pinned order with one row each
//! under the pass's one correlation id.

use std::sync::Arc;

use astra_ai::{DlpPolicy, JudgeVerdict};

use super::{fakes, first, passed_app, rule, stages, store_with, unread_mailbox, PRIMARY};

#[tokio::test]
async fn every_stage_runs_in_order_under_the_pass_correlation() {
    let store = store_with(&[rule("Newsletter")]);
    // The match says no, the cold judge says yes: the order the seams hear
    // the calls in is the pinned stage order.
    let judge =
        fakes::ai::FixtureJudge::scripted(&[first(JudgeVerdict::No), first(JudgeVerdict::Yes)]);
    let complete = fakes::ai::FixtureComplete::scripted(&[r#"{"category":"newsletter"}"#]);
    let (app, judge_probe, complete_probe, _) = passed_app(
        unread_mailbox(1),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    let use_cases: Vec<_> = judge_probe
        .calls()
        .iter()
        .map(|call| call.use_case)
        .collect();
    assert_eq!(
        use_cases,
        vec![
            astra_ai::JudgeUseCase::JudgeRuleMatch,
            astra_ai::JudgeUseCase::JudgeColdEmail,
        ],
        "the rule match precedes the cold judge",
    );
    assert_eq!(complete_probe.call_count(), 1, "categorise ran once");
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(rows.len(), 3, "one row per stage");
    for row in &rows {
        assert_eq!(
            row.correlation, rows[0].correlation,
            "every stage row rides the pass's one correlation id",
        );
    }
    assert_eq!(
        stages(&store, "m1"),
        vec![
            ("cold".to_owned(), "yes".to_owned()),
            ("categorise".to_owned(), "ok".to_owned()),
            ("rule_match".to_owned(), "no".to_owned()),
        ],
        "the rows land newest first in stage order",
    );
    let categories: Vec<_> = rows.iter().map(|row| row.category.as_deref()).collect();
    assert_eq!(
        categories,
        vec![None, Some("newsletter"), None],
        "only the categorise row carries the category",
    );
}

#[tokio::test]
async fn a_blocking_dlp_policy_refuses_the_message_s_stages() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let (app, judge_probe, complete_probe, edits) = passed_app(
        unread_mailbox(1),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Block,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        judge_probe.call_count(),
        0,
        "the gate refused before any judge call was spent",
    );
    assert_eq!(
        complete_probe.call_count(),
        0,
        "the gate refused before any completion call was spent",
    );
    assert_eq!(
        stages(&store, "m1"),
        vec![("dlp".to_owned(), "blocked".to_owned())],
        "the refusal is one honestly blocked row",
    );
    assert!(
        edits.lock().unwrap().is_empty(),
        "a blocked message executes nothing, even a rule that would match",
    );
}

#[tokio::test]
async fn a_redacting_dlp_policy_masks_before_any_prompt() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Pitch"),
            8,
            0,
        )],
        vec![(
            "m1".to_owned(),
            b"From: ada@example.test\r\nSubject: Pitch\r\n\r\n\
              Reach bob@corp.test or call +1 (555) 123-4567 today.\r\n"
                .to_vec(),
        )],
    );
    let (app, judge_probe, complete_probe, _) = passed_app(
        provider,
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Redact,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        complete_probe.call_count(),
        1,
        "redaction allows the stages, masked"
    );
    for call in judge_probe.calls() {
        assert!(
            !call.context.contains("bob@corp.test"),
            "the address never rides a prompt:\n{}",
            call.context,
        );
        assert!(
            !call.context.contains("123-4567") && !call.context.contains("ada@example.test"),
            "phones and senders ride masked:\n{}",
            call.context,
        );
    }
    let prompt = &complete_probe.calls()[0].prompt;
    assert!(
        !prompt.contains("bob@corp.test") && !prompt.contains("123-4567"),
        "the categorise prompt rides masked:\n{prompt}",
    );
    assert_eq!(
        stages(&store, "m1"),
        vec![
            ("cold".to_owned(), "no".to_owned()),
            ("categorise".to_owned(), "ok".to_owned()),
            ("rule_match".to_owned(), "no".to_owned()),
        ],
        "an allowed gate records no row of its own",
    );
}
