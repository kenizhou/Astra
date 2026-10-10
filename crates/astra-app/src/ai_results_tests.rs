// SPDX-License-Identifier: GPL-3.0-only
//! Incoming pass tests: the bounded sweep over newly seen unread mail,
//! over the fixture judge, the fixture completion, and the real write-path
//! fakes. The head limit is honoured newest first; a message with a
//! recorded result is skipped (the durable 0003 row is the seen tracking);
//! read mail is never a candidate; the message-level stages run with no
//! rules at all; the abstain default executes nothing while a yes runs the
//! rule's actions audited; a refresh arms the pass without running it on
//! the refresh's own path; and the driver runs what a refresh armed. The
//! stage-order and dlp-gate tests live in the `dlp_gate` child module
//! (their own file, so this one stays under the line cap).

use std::sync::{Arc, Mutex};

use astra_ai::{
    AiStore, CorrelationId, DlpPolicy, JudgeOutcome, JudgePath, JudgeVerdict, NewAiResult, RuleId,
};
use engine_provider::MailEdit;

use crate::ai_rules::{Action, Rule};
use crate::dispatch_fakes as fakes;
use crate::dispatch_fakes::ai::app_with_ai;
use crate::{App, Intent};

/// The pass's head limit, so the fixtures can overrun it.
const LIMIT: usize = crate::ai_results::PASS_MESSAGE_LIMIT;

/// The fixture categorisation answer: a primary message.
const PRIMARY: &str = r#"{"category":"primary"}"#;

/// A first-try verdict outcome.
fn first(verdict: JudgeVerdict) -> JudgeOutcome {
    JudgeOutcome {
        verdict,
        path: JudgePath::FirstTry,
    }
}

/// The abstain default outcome (verdict No, the table's answer).
fn abstain_default() -> JudgeOutcome {
    JudgeOutcome {
        verdict: JudgeVerdict::No,
        path: JudgePath::AbstainDefault,
    }
}

/// One enabled rule with both actions of the closed set.
fn rule(name: &str) -> Rule {
    Rule {
        id: RuleId::new(format!("rule-{name}")),
        name: name.to_owned(),
        condition: format!("The message is a {name} mail"),
        actions: vec![Action::MarkRead, Action::SetFlagged],
        enabled: true,
    }
}

/// A store holding `rules`, open in memory.
fn store_with(rules: &[Rule]) -> Arc<AiStore> {
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    for one in rules {
        store.create_rule(&one.as_new_rule()).unwrap();
    }
    store
}

/// `count` unread messages `m1..`, each delivered an hour apart with a
/// plain source, so the newest-first order is fixed and every candidate has
/// a body to read.
fn unread_mailbox(count: usize) -> fakes::FakeProvider {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for at in 1..=count {
        let key = format!("m{at}");
        let hour = u8::try_from(7 + at).unwrap();
        messages.push(fakes::delivered_at(
            fakes::mail(&key, "inbox", "Launch"),
            hour,
            0,
        ));
        sources.push((
            key.clone(),
            format!("From: ada@example.test\r\nSubject: Launch\r\n\r\nBody of {key}.\r\n")
                .into_bytes(),
        ));
    }
    fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources)
}

/// An app over the fixture provider with the AI pass wired over `store`,
/// the two seams and the dlp policy, synced once so the engine holds the
/// mail, beside the probes and the provider's edit log.
async fn passed_app(
    provider: fakes::FakeProvider,
    judge: fakes::ai::FixtureJudge,
    complete: fakes::ai::FixtureComplete,
    store: Arc<AiStore>,
    dlp: DlpPolicy,
) -> (
    App<fakes::FakeProvider>,
    fakes::ai::FixtureJudge,
    fakes::ai::FixtureComplete,
    Arc<Mutex<Vec<MailEdit>>>,
) {
    let edits = Arc::clone(&provider.edits);
    let judge_probe = judge.clone();
    let complete_probe = complete.clone();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        judge,
        complete,
        store,
        dlp,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, judge_probe, complete_probe, edits)
}

/// The stage and verdict words recorded for one message, newest first.
fn stages(store: &AiStore, key: &str) -> Vec<(String, String)> {
    store
        .ai_results_for_message("work", key)
        .expect("the results read")
        .into_iter()
        .map(|row| (row.stage, row.verdict))
        .collect()
}

#[tokio::test]
async fn the_pass_judges_the_head_limit_of_the_newest_unread_mail() {
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let store = store_with(&[rule("Newsletter")]);
    let (app, _, complete_probe, _) = passed_app(
        unread_mailbox(LIMIT + 3),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        complete_probe.call_count(),
        LIMIT,
        "the pass categorised exactly its bound"
    );
    // The newest messages are the head: with LIMIT + 3 candidates the three
    // oldest wait for a later pass.
    for oldest in 1..=3 {
        assert!(
            stages(&store, &format!("m{oldest}")).is_empty(),
            "m{oldest} is below the head and was not judged",
        );
    }
    for judged in 4..=(LIMIT + 3) {
        assert_eq!(
            stages(&store, &format!("m{judged}")),
            vec![
                ("cold".to_owned(), "no".to_owned()),
                ("categorise".to_owned(), "ok".to_owned()),
                ("rule_match".to_owned(), "no".to_owned()),
            ],
            "m{judged} was judged once, every stage its own row",
        );
    }
}

#[tokio::test]
async fn a_message_with_a_recorded_result_is_not_rejudged() {
    let store = store_with(&[rule("Newsletter")]);
    // The newest message already has a result row: it is seen, whatever
    // pass recorded it and whatever it said.
    store
        .record_ai_result(&NewAiResult {
            correlation: CorrelationId::generate(),
            message_account: "work".to_owned(),
            message_key: "m2".to_owned(),
            stage: "rule_match".to_owned(),
            rule_id: None,
            verdict: "no".to_owned(),
            category: None,
        })
        .unwrap();
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let (app, judge_probe, complete_probe, _) = passed_app(
        unread_mailbox(2),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        judge_probe.call_count(),
        2,
        "only the unseen message was judged (its match and its cold call)",
    );
    assert_eq!(complete_probe.call_count(), 1);
    assert_eq!(
        stages(&store, "m2").len(),
        1,
        "the seen message still carries only its seeded row",
    );
    assert_eq!(
        stages(&store, "m1"),
        vec![
            ("cold".to_owned(), "no".to_owned()),
            ("categorise".to_owned(), "ok".to_owned()),
            ("rule_match".to_owned(), "no".to_owned()),
        ],
        "the unseen one was judged and recorded",
    );
}

#[tokio::test]
async fn the_second_pass_moves_past_what_the_first_judged() {
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let store = store_with(&[rule("Newsletter")]);
    let (app, _, complete_probe, _) = passed_app(
        unread_mailbox(3),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;
    app.run_incoming_pass().await;
    let first_count = complete_probe.call_count();

    app.run_incoming_pass().await;

    assert_eq!(
        complete_probe.call_count(),
        first_count,
        "a second pass rejudges nothing the first recorded",
    );
    for at in 1..=3 {
        assert_eq!(
            stages(&store, &format!("m{at}")).len(),
            3,
            "one row per stage per message, from one pass",
        );
    }
}

#[tokio::test]
async fn the_pass_runs_matched_actions_through_the_write_paths_audited() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let (app, _, _, edits) = passed_app(
        unread_mailbox(1),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [
            MailEdit::mark_seen(engine_api::ProviderKey::new("m1").unwrap(), true),
            MailEdit::set_flagged(engine_api::ProviderKey::new("m1").unwrap(), true),
        ],
        "the matched rule's actions ran through the deterministic paths",
    );
    let rows = store.ai_results_for_message("work", "m1").expect("reads");
    assert_eq!(rows.len(), 3, "every stage landed its row");
    let audits = store
        .audit_rows_for_correlation(&rows[0].correlation)
        .expect("the audit rows read");
    assert_eq!(
        audits.len(),
        2,
        "both executions carry the pass's one correlation id",
    );
}

#[tokio::test]
async fn the_abstain_default_executes_nothing() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(abstain_default());
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let (app, _, _, edits) = passed_app(
        unread_mailbox(1),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    assert!(
        edits.lock().unwrap().is_empty(),
        "the safe default is no match, so no write ran",
    );
    assert_eq!(
        stages(&store, "m1"),
        vec![
            ("cold".to_owned(), "abstain_default".to_owned()),
            ("categorise".to_owned(), "ok".to_owned()),
            ("rule_match".to_owned(), "abstain_default".to_owned()),
        ],
        "the defaults are recorded honestly, and the abstaining cold judge is not cold",
    );
}

#[tokio::test]
async fn read_messages_are_never_candidates() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let read = fakes::read(fakes::delivered_at(fakes::mail("m1", "inbox", "Old"), 8, 0));
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![read],
        vec![(
            "m1".to_owned(),
            b"From: a@example.test\r\nSubject: Old\r\n\r\nRead already.\r\n".to_vec(),
        )],
    );
    let (app, judge_probe, complete_probe, edits) = passed_app(
        provider,
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        judge_probe.call_count(),
        0,
        "already-read mail is not incoming mail"
    );
    assert_eq!(complete_probe.call_count(), 0);
    assert!(edits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn without_enabled_rules_the_match_stage_stays_empty() {
    let store = store_with(&[]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let (app, judge_probe, complete_probe, _) = passed_app(
        unread_mailbox(2),
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    )
    .await;

    app.run_incoming_pass().await;

    assert_eq!(
        complete_probe.call_count(),
        2,
        "the message-level stages run with no rules at all",
    );
    let use_cases: Vec<_> = judge_probe
        .calls()
        .iter()
        .map(|call| call.use_case)
        .collect();
    assert_eq!(
        use_cases,
        vec![
            astra_ai::JudgeUseCase::JudgeColdEmail,
            astra_ai::JudgeUseCase::JudgeColdEmail,
        ],
        "the judge hears only the cold calls, no rule to match",
    );
    for at in 1..=2 {
        assert_eq!(
            stages(&store, &format!("m{at}")),
            vec![
                ("cold".to_owned(), "no".to_owned()),
                ("categorise".to_owned(), "ok".to_owned()),
            ],
            "no rule_match row exists",
        );
    }
}

#[tokio::test]
async fn a_refresh_arms_the_pass_without_running_it_on_the_refresh_path() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let provider = unread_mailbox(1);
    let edits = Arc::clone(&provider.edits);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        judge,
        complete,
        store,
        DlpPolicy::Allow,
    );
    // No driver spawned: whatever runs, runs on the caller's own path.
    let armed = std::pin::pin!(app.pass_armed.notified());

    app.dispatch(Intent::RefreshMail).await.unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(5), armed)
        .await
        .expect("the refresh armed the pass");
    assert!(
        edits.lock().unwrap().is_empty(),
        "the refresh itself judged and wrote nothing",
    );
}

#[tokio::test]
async fn the_driver_runs_what_a_refresh_armed() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let complete = fakes::ai::FixtureComplete::answering(PRIMARY);
    let provider = unread_mailbox(1);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = Arc::new(app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        judge,
        complete,
        Arc::clone(&store),
        DlpPolicy::Allow,
    ));
    let driver = Arc::clone(&app);
    tokio::spawn(async move {
        driver.drive_incoming_pass().await;
    });

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let mut ran = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if !stages(&store, "m1").is_empty() {
            ran = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    assert!(
        ran,
        "the armed pass ran on the driver, off the refresh path",
    );
}

// The stage-order and dlp-gate tests, a child module so they share this
// module's fixtures while keeping their own file under the cap.
#[path = "ai_results_dlp_tests.rs"]
mod dlp_gate;
