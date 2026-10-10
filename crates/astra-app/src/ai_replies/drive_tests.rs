// SPDX-License-Identifier: GPL-3.0-only
//! Reply-drafting driver tests: an open arms the generation off its own
//! path, the driver drafts once per message (pool presence, skipped rows
//! included, the arming's correlation riding or a fresh one minted), the
//! thread history, the style note, and the knowledge context ride the
//! prompt, and a landed row signals the host's sink. The host-pull tests
//! live in the `pulls` child module (their own file, so this one stays
//! under the line cap).

use std::sync::{Arc, Mutex};

use astra_ai::{
    AiStore, CorrelationId, DlpPolicy, JudgeOutcome, JudgePath, JudgeVerdict, KnowledgeId,
    NewKnowledgeEntry,
};
use astra_viewmodel::MessageKey;

use crate::ai_replies::DraftPoolSink;
use crate::dispatch_fakes as fakes;
use crate::dispatch_fakes::ai::app_with_drafting;
use crate::reference::MessageRef;
use crate::{App, Intent};

/// The fixture draft answer: one medium-confidence reply.
const DRAFT: &str = r#"{"reply":"Sounds good, shipped it.","confidence":"medium"}"#;

/// The fixture style answer: one short note.
const STYLE: &str = r#"{"style_note":"Crisp and warm, signs with thanks."}"#;

/// A first-try verdict outcome, the judge fixtures' shape.
fn yes() -> JudgeOutcome {
    JudgeOutcome {
        verdict: JudgeVerdict::Yes,
        path: JudgePath::FirstTry,
    }
}

/// A message reference for the fixture account.
fn message(key: &str) -> MessageRef {
    MessageRef {
        account: engine_api::AccountId::try_from("work").unwrap(),
        key: engine_api::ProviderKey::new(key).unwrap(),
    }
}

/// A plain source for `key`, so the reading and drafting reads have a body.
fn source(key: &str) -> (String, Vec<u8>) {
    (
        key.to_owned(),
        format!("From: ada@example.test\r\nSubject: Re: Launch\r\n\r\nBody of {key}.\r\n")
            .into_bytes(),
    )
}

/// A three-message thread `t-1` in the account's Inbox: `m1` the oldest,
/// `m2`, and `m3` the newest (the one a reading open names), each with a
/// plain source so bodies resolve.
fn thread() -> fakes::FakeProvider {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for (at, hour) in [(1, 8), (2, 9), (3, 10)] {
        let key = format!("m{at}");
        messages.push(fakes::threaded(
            fakes::delivered_at(fakes::mail(&key, "inbox", "Re: Launch"), hour, 0),
            "t-1",
        ));
        sources.push(source(&key));
    }
    fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources)
}

/// An app over `provider` with the drafting bundle wired over `store` and
/// `sink`, synced once so the store holds the mail. The judge answers Yes;
/// the driver never asks it, but the bundle demands one.
async fn drafted_app(
    provider: fakes::FakeProvider,
    complete: fakes::ai::FixtureComplete,
    store: Arc<AiStore>,
    sink: Option<Arc<dyn DraftPoolSink>>,
) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_drafting(
        vec![fakes::account("work", provider)],
        &surfaces,
        fakes::ai::FixtureJudge::answering(yes()),
        complete,
        store,
        DlpPolicy::Allow,
        sink,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// A sink that records the messages it was told landed, the assertions'
/// witness for the row-landing signal.
#[derive(Default)]
struct RecordingSink {
    /// The (account, key) pairs that landed a pool row, in order.
    landed: Mutex<Vec<(String, String)>>,
}

impl DraftPoolSink for RecordingSink {
    fn draft_pool_landed(&self, message: MessageKey) {
        self.landed.lock().unwrap().push((
            message.account.as_str().to_owned(),
            message.key.as_str().to_owned(),
        ));
    }
}

/// The recording sink as the bundle holds it, the unsize coercion the
/// `Option<Arc<dyn …>>` parameter demands.
fn sink_of(sink: &Arc<RecordingSink>) -> Arc<dyn DraftPoolSink> {
    let held: Arc<dyn DraftPoolSink> = sink.clone();
    held
}

/// The pool rows recorded for one message, newest first.
fn rows(store: &AiStore, key: &str) -> Vec<(Option<String>, Option<f64>)> {
    store
        .draft_pool_for_message("work", key)
        .expect("the pool read")
        .into_iter()
        .map(|row| (row.reply, row.confidence))
        .collect()
}

/// Waits until `check` holds, polling on the runtime's clock; the driver
/// runs on its own task, so the test meets it where it lands.
async fn wait_until(mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !check() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    assert!(check(), "the condition settled within its deadline");
}

#[tokio::test]
async fn an_open_arms_off_its_own_path_and_the_driver_drafts_once_per_message() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let probe = complete.clone();
    let sink = Arc::new(RecordingSink::default());
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(thread(), complete, Arc::clone(&store), Some(sink_of(&sink))).await;

    app.dispatch(Intent::OpenMessage {
        message: message("m3"),
    })
    .await
    .unwrap();
    assert_eq!(
        probe.call_count(),
        0,
        "the open armed the generation without running it on its own path",
    );
    assert!(rows(&store, "m3").is_empty(), "nothing drafted yet");

    app.run_reply_drafting().await;

    assert_eq!(
        probe.call_count(),
        1,
        "the driver drafted the armed message"
    );
    assert_eq!(
        rows(&store, "m3"),
        vec![(Some("Sounds good, shipped it.".to_owned()), Some(0.6))],
        "the drafted row landed",
    );
    assert_eq!(
        sink.landed.lock().unwrap().as_slice(),
        [("work".to_owned(), "m3".to_owned())].as_slice(),
        "the landed row signalled the sink",
    );

    // A re-open of a message the pool already holds (skipped rows included)
    // arms nothing the driver would spend again.
    app.dispatch(Intent::OpenMessage {
        message: message("m3"),
    })
    .await
    .unwrap();
    app.run_reply_drafting().await;
    assert_eq!(
        probe.call_count(),
        1,
        "one draft per message, pool presence the dedup",
    );
    assert_eq!(rows(&store, "m3").len(), 1);
    assert_eq!(sink.landed.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_failed_generation_records_a_skipped_row_and_is_never_retried() {
    let complete = fakes::ai::FixtureComplete::failing("no route to host");
    let probe = complete.clone();
    let sink = Arc::new(RecordingSink::default());
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(thread(), complete, Arc::clone(&store), Some(sink_of(&sink))).await;

    app.request_reply_draft(message("m3"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;

    assert_eq!(
        rows(&store, "m3"),
        vec![(None, None)],
        "the failed generation recorded its skipped row",
    );
    assert_eq!(
        sink.landed.lock().unwrap().as_slice(),
        [("work".to_owned(), "m3".to_owned())].as_slice(),
        "a skipped row lands no less, the pool's presence changed",
    );

    app.request_reply_draft(message("m3"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;
    assert_eq!(
        probe.call_count(),
        1,
        "a skipped row is pool presence; a later open spends nothing",
    );
    assert_eq!(rows(&store, "m3").len(), 1);
}

#[tokio::test]
async fn a_message_whose_context_cannot_be_collected_skips_the_generation() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let probe = complete.clone();
    let sink = Arc::new(RecordingSink::default());
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    // The message resolves but holds no source, so the body fetch fails and
    // the collector answers nothing.
    let mut messages = Vec::new();
    for (at, hour) in [(1, 8), (3, 10)] {
        let key = format!("m{at}");
        messages.push(fakes::threaded(
            fakes::delivered_at(fakes::mail(&key, "inbox", "Re: Launch"), hour, 0),
            "t-1",
        ));
    }
    let provider =
        fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, vec![source("m1")]);
    let app = drafted_app(provider, complete, Arc::clone(&store), Some(sink_of(&sink))).await;

    app.request_reply_draft(message("m3"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;

    assert_eq!(probe.call_count(), 0, "no body, no draft, no call spent");
    assert!(rows(&store, "m3").is_empty(), "nothing was recorded");
    assert!(
        sink.landed.lock().unwrap().is_empty(),
        "no row landed, so nothing signalled",
    );
}

#[tokio::test]
async fn the_armings_correlation_rides_and_an_absent_one_is_minted() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(thread(), complete, Arc::clone(&store), None).await;

    let known = CorrelationId::new("corr-open-turn".to_owned());
    app.request_reply_draft(message("m3"), Some(known.clone()));
    app.run_reply_drafting().await;
    assert_eq!(
        probe.calls()[0].correlation,
        known,
        "the reading turn's correlation rode the generation",
    );
    assert_eq!(
        rows_correlation(&store, "m3"),
        vec![known],
        "the landed row carries the same id",
    );

    app.request_reply_draft(message("m2"), None);
    app.run_reply_drafting().await;
    let minted = &probe.calls()[1].correlation;
    assert_ne!(
        minted,
        &probe.calls()[0].correlation,
        "an arming with no correlation of its own gets a fresh one",
    );
    assert!(
        !minted.as_str().is_empty(),
        "the minted id is a real correlation id",
    );
}

#[tokio::test]
async fn the_thread_history_rides_oldest_first_with_the_original_last() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(thread(), complete, store, None).await;

    app.request_reply_draft(message("m3"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;

    let prompt = &probe.calls()[0].prompt;
    let m1 = prompt.find("Body of m1.").expect("the oldest member rides");
    let m2 = prompt.find("Body of m2.").expect("the middle member rides");
    let m3 = prompt
        .find("Body of m3.")
        .expect("the original rides, and last");
    assert!(
        m1 < m2 && m2 < m3,
        "the history renders oldest first and the original closes the thread",
    );
}

#[tokio::test]
async fn a_threadless_original_drafts_from_itself_alone() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let solo = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("solo", "inbox", "Launch"),
            10,
            0,
        )],
        vec![source("solo")],
    );
    let app = drafted_app(solo, complete, store, None).await;

    app.request_reply_draft(message("solo"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;

    let prompt = &probe.calls()[0].prompt;
    assert!(prompt.contains("Body of solo."), "the original rides");
    assert_eq!(
        prompt.matches("Body of").count(),
        1,
        "a threadless message is the whole thread",
    );
}

#[tokio::test]
async fn the_style_note_is_inferred_once_and_rides_every_draft_prompt() {
    // One Sent message beside the thread, so the collector has samples to
    // infer from; the completion answers the infer then the draft.
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for (at, hour) in [(1, 8), (2, 9), (3, 10)] {
        let key = format!("m{at}");
        messages.push(fakes::threaded(
            fakes::delivered_at(fakes::mail(&key, "inbox", "Re: Launch"), hour, 0),
            "t-1",
        ));
        sources.push(source(&key));
    }
    messages.push(fakes::delivered_at(
        fakes::mail("s1", "sent", "Shipped"),
        7,
        0,
    ));
    sources.push(source("s1"));
    let provider =
        fakes::FakeProvider::with_sources(vec![fakes::inbox(), fakes::sent()], messages, sources);
    let complete = fakes::ai::FixtureComplete::scripted(&[STYLE, DRAFT, DRAFT]);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(provider, complete, Arc::clone(&store), None).await;

    app.request_reply_draft(message("m3"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;
    let calls = probe.calls();
    assert_eq!(
        calls[0].use_case,
        astra_ai::LlmUseCase::StyleInfer,
        "the first use of an account infers its style note",
    );
    assert_eq!(calls[1].use_case, astra_ai::LlmUseCase::DraftReply);
    assert!(
        calls[1]
            .prompt
            .contains("Crisp and warm, signs with thanks."),
        "the note rides the draft prompt as data",
    );
    assert!(
        store
            .style_profile("work")
            .expect("the profile read")
            .is_some(),
        "the inferred profile was saved",
    );

    // The daily cap: a second generation the same day reads the stored note
    // and spends no infer call.
    app.request_reply_draft(message("m2"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;
    let calls = probe.calls();
    assert_eq!(
        calls[2].use_case,
        astra_ai::LlmUseCase::DraftReply,
        "the fresh profile answered from the store",
    );
    assert!(
        calls[2]
            .prompt
            .contains("Crisp and warm, signs with thanks."),
        "the stored note rides just the same",
    );
}

#[tokio::test]
async fn the_account_knowledge_rides_every_draft_prompt() {
    // Two stored entries beside the thread, so the collector's read has
    // lines to shape with; the completion answers the draft alone (no
    // Sent messages, so the style never infers).
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    for (kind, text) in [
        (
            "persona",
            "The user writes plainly and signs off with Cheers.",
        ),
        ("fact", "The launch lands on Friday."),
    ] {
        store
            .insert_knowledge(&NewKnowledgeEntry {
                id: KnowledgeId::new(format!("k-{kind}")),
                account: "work".to_owned(),
                kind: kind.to_owned(),
                text: text.to_owned(),
                source_message_account: "work".to_owned(),
                source_message_key: "m0".to_owned(),
                correlation: CorrelationId::new("c-old".to_owned()),
            })
            .unwrap();
    }
    let complete = fakes::ai::FixtureComplete::scripted(&[DRAFT]);
    let probe = complete.clone();
    let app = drafted_app(thread(), complete, Arc::clone(&store), None).await;

    app.request_reply_draft(message("m3"), Some(CorrelationId::generate()));
    app.run_reply_drafting().await;
    let calls = probe.calls();
    assert_eq!(calls[0].use_case, astra_ai::LlmUseCase::DraftReply);
    assert!(
        calls[0].prompt.contains("Known facts about the user"),
        "the knowledge section rides the draft prompt:\n{}",
        calls[0].prompt,
    );
    assert!(
        calls[0]
            .prompt
            .contains("fact: The launch lands on Friday."),
        "the stored entries ride as labelled lines:\n{}",
        calls[0].prompt,
    );
}

#[tokio::test]
async fn the_driver_runs_what_an_open_armed() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = Arc::new(drafted_app(thread(), complete, Arc::clone(&store), None).await);
    let driver = Arc::clone(&app);
    tokio::spawn(async move {
        driver.drive_reply_drafting().await;
    });

    app.dispatch(Intent::OpenMessage {
        message: message("m3"),
    })
    .await
    .unwrap();

    wait_until(|| !rows(&store, "m3").is_empty()).await;
}

/// The correlations of the pool rows recorded for one message, newest
/// first: the id the landed row carries.
fn rows_correlation(store: &AiStore, key: &str) -> Vec<CorrelationId> {
    store
        .draft_pool_for_message("work", key)
        .expect("the pool read")
        .into_iter()
        .map(|row| row.correlation)
        .collect()
}

// The host-pull tests (the chips, summarise, style, and dismiss commands
// the desktop surface drives), a child module so they share this module's
// fixtures while keeping their own file under the cap.
#[path = "drive_pull_tests.rs"]
mod pulls;
