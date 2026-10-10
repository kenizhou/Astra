// SPDX-License-Identifier: GPL-3.0-only
//! Incoming pass tests: the bounded sweep over newly seen unread mail,
//! over the fixture judge and the real write-path fakes. The head limit is
//! honoured newest first; a message with a recorded result is skipped (the
//! durable 0003 row is the seen tracking); read mail is never a candidate;
//! the abstain default executes nothing while a yes runs the rule's
//! actions audited under the pass's one correlation id; a refresh arms the
//! pass without running it on the refresh's own path; and the driver runs
//! what a refresh armed.

use std::sync::{Arc, Mutex};

use astra_ai::{
    AiStore, CorrelationId, JudgeOutcome, JudgePath, JudgeVerdict, NewAiResult, RuleId,
};
use engine_provider::MailEdit;

use crate::ai_rules::{Action, Rule};
use crate::dispatch_fakes as fakes;
use crate::dispatch_fakes::ai::app_with_ai;
use crate::{App, Intent};

/// The pass's head limit, so the fixtures can overrun it.
const LIMIT: usize = crate::ai_results::PASS_MESSAGE_LIMIT;

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
/// synced once so the engine holds the mail, beside the judge probe and
/// the provider's edit log.
async fn passed_app(
    provider: fakes::FakeProvider,
    judge: fakes::ai::FixtureJudge,
    store: Arc<AiStore>,
) -> (
    App<fakes::FakeProvider>,
    fakes::ai::FixtureJudge,
    Arc<Mutex<Vec<MailEdit>>>,
) {
    let edits = Arc::clone(&provider.edits);
    let probe = judge.clone();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        judge,
        store,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, probe, edits)
}

/// The rule_match verdict words recorded for one message.
fn verdicts(store: &AiStore, key: &str) -> Vec<String> {
    store
        .ai_results_for_message("work", key)
        .expect("the results read")
        .into_iter()
        .map(|row| row.verdict)
        .collect()
}

#[tokio::test]
async fn the_pass_judges_the_head_limit_of_the_newest_unread_mail() {
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let store = store_with(&[rule("Newsletter")]);
    let (app, probe, _) = passed_app(unread_mailbox(LIMIT + 3), judge, Arc::clone(&store)).await;

    app.run_incoming_pass().await;

    assert_eq!(
        probe.call_count(),
        LIMIT,
        "the pass judged exactly its bound"
    );
    // The newest messages are the head: with LIMIT + 3 candidates the three
    // oldest wait for a later pass.
    for oldest in 1..=3 {
        assert!(
            verdicts(&store, &format!("m{oldest}")).is_empty(),
            "m{oldest} is below the head and was not judged",
        );
    }
    for judged in 4..=(LIMIT + 3) {
        assert_eq!(
            verdicts(&store, &format!("m{judged}")),
            vec!["no".to_owned()],
            "m{judged} was judged once",
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
    let (app, probe, _) = passed_app(unread_mailbox(2), judge, Arc::clone(&store)).await;

    app.run_incoming_pass().await;

    assert_eq!(probe.call_count(), 1, "only the unseen message was judged");
    assert_eq!(
        verdicts(&store, "m2").len(),
        1,
        "the seen message still carries only its seeded row",
    );
    assert_eq!(
        verdicts(&store, "m1"),
        vec!["no".to_owned()],
        "the unseen one was judged and recorded",
    );
}

#[tokio::test]
async fn the_second_pass_moves_past_what_the_first_judged() {
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::No));
    let store = store_with(&[rule("Newsletter")]);
    let (app, probe, _) = passed_app(unread_mailbox(3), judge, Arc::clone(&store)).await;
    app.run_incoming_pass().await;
    let first_count = probe.call_count();

    app.run_incoming_pass().await;

    assert_eq!(
        probe.call_count(),
        first_count,
        "a second pass rejudges nothing the first recorded",
    );
    for at in 1..=3 {
        assert_eq!(
            verdicts(&store, &format!("m{at}")).len(),
            1,
            "one result row per message, from one pass",
        );
    }
}

#[tokio::test]
async fn the_pass_runs_matched_actions_through_the_write_paths_audited() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let (app, _, edits) = passed_app(unread_mailbox(1), judge, Arc::clone(&store)).await;

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
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].verdict, "yes");
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
    let (app, _, edits) = passed_app(unread_mailbox(1), judge, Arc::clone(&store)).await;

    app.run_incoming_pass().await;

    assert!(
        edits.lock().unwrap().is_empty(),
        "the safe default is no match, so no write ran",
    );
    assert_eq!(
        verdicts(&store, "m1"),
        vec!["abstain_default".to_owned()],
        "the default is recorded honestly",
    );
}

#[tokio::test]
async fn read_messages_are_never_candidates() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let read = fakes::read(fakes::delivered_at(fakes::mail("m1", "inbox", "Old"), 8, 0));
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![read],
        vec![(
            "m1".to_owned(),
            b"From: a@example.test\r\nSubject: Old\r\n\r\nRead already.\r\n".to_vec(),
        )],
    );
    let (app, probe, edits) = passed_app(provider, judge, Arc::clone(&store)).await;

    app.run_incoming_pass().await;

    assert_eq!(
        probe.call_count(),
        0,
        "already-read mail is not incoming mail"
    );
    assert!(edits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn without_enabled_rules_the_pass_judges_nothing() {
    let store = store_with(&[]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let (app, probe, _) = passed_app(unread_mailbox(2), judge, Arc::clone(&store)).await;

    app.run_incoming_pass().await;

    assert_eq!(probe.call_count(), 0, "no rules, no calls");
}

#[tokio::test]
async fn a_refresh_arms_the_pass_without_running_it_on_the_refresh_path() {
    let store = store_with(&[rule("Newsletter")]);
    let judge = fakes::ai::FixtureJudge::answering(first(JudgeVerdict::Yes));
    let provider = unread_mailbox(1);
    let edits = Arc::clone(&provider.edits);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        judge,
        store,
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
    let provider = unread_mailbox(1);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = Arc::new(app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        judge,
        Arc::clone(&store),
    ));
    let driver = Arc::clone(&app);
    tokio::spawn(async move {
        driver.drive_incoming_pass().await;
    });

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let mut ran = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if !verdicts(&store, "m1").is_empty() {
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
