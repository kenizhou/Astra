// SPDX-License-Identifier: GPL-3.0-only
//! AI documents driver tests: the generation pull collects the window's
//! mail newest first so the head-five each category rides are the newest
//! five, a body that cannot be fetched skips its message whole, the same
//! period's second generation replaces the stored row under the
//! regenerated flag, the report's sent count comes from the Sent
//! mailbox's rows inside the window, and the window bounds quantise to
//! whole UTC days behind the clamp. The split and quiet-answer pulls
//! ride the child module the file's tail declares, the commands tests'
//! own split for the same line cap.

use std::sync::{Arc, Mutex};

use astra_ai::{
    AiDocumentKind, AiStore, CorrelationId, JudgeOutcome, JudgePath, JudgeVerdict, NewAiResult,
};

use super::super::{SplitSide, DOCUMENT_HEAD_LIMIT, DOCUMENT_WINDOW_MAX_DAYS};
use super::{
    window_bounds, DigestView, DocumentError, DocumentView, SplitEntry,
    DOCUMENT_WINDOW_DEFAULT_DAYS, DOCUMENT_WINDOW_READ_LIMIT,
};
use crate::dispatch_fakes as fakes;
use crate::dispatch_fakes::ai::app_with_ai;
use crate::{App, Intent};

/// The digest fixture answer: the one-field shape the output policy pins.
const DIGESTED: &str = r#"{"digest":"Two replies wait; the newsletter can wait."}"#;

/// A second digest answer, so a regeneration's replace is content the
/// test can see move.
const REDIGESTED: &str = r#"{"digest":"All quiet now."}"#;

/// The report fixture answer: the three-field shape the output policy
/// pins, assembled under the section headings.
const REPORTED: &str = r#"{"executive_summary":"A launch week.",
    "response_patterns":"Replies cluster in the morning.",
    "recommendations":"Batch the newsletters."}"#;

/// The instant every generation turn fixes as its window's "now": the
/// last second of the fixture day, so a one-day window covers exactly
/// that day and a seven-day window the week behind it.
fn day_end() -> i64 {
    time::OffsetDateTime::new_utc(
        time::Date::from_calendar_date(2026, time::Month::October, 9).unwrap(),
        time::Time::from_hms(23, 59, 59).unwrap(),
    )
    .unix_timestamp()
}

/// The unix second of one civil UTC instant on the fixture day.
fn at(hour: u8, minute: u8) -> i64 {
    time::OffsetDateTime::new_utc(
        time::Date::from_calendar_date(2026, time::Month::October, 9).unwrap(),
        time::Time::from_hms(hour, minute, 0).unwrap(),
    )
    .unix_timestamp()
}

/// The fixture account every pull names.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").unwrap()
}

/// A first-try verdict outcome, the bundle's unused judge demand.
fn yes() -> JudgeOutcome {
    JudgeOutcome {
        verdict: JudgeVerdict::Yes,
        path: JudgePath::FirstTry,
    }
}

/// A plain source for `key`, so the body reads have bytes to decode.
fn source(key: &str) -> (String, Vec<u8>) {
    (
        key.to_owned(),
        format!("From: ada@example.test\r\nSubject: Re: Note\r\n\r\nBody of {key}.\r\n")
            .into_bytes(),
    )
}

/// One inbox message `key` about `subject`, delivered at `hour:00` on
/// the fixture day.
fn inbox_mail(key: &str, subject: &str, hour: u8) -> engine_api::Message {
    fakes::delivered_at(fakes::mail(key, "inbox", subject), hour, 0)
}

/// Redelivers `message` on the first of the month instead, so it sits
/// outside a window over the fixture day.
fn on_the_first(message: engine_api::Message) -> engine_api::Message {
    let mut message = message;
    message.received_at = Some(engine_api::UtcDateTime::new(2026, 10, 1, 8, 0, 0).unwrap());
    message
}

/// Records one decided categorise row for `key`, the shape the incoming
/// pass lands for every message it judges.
fn categorised(store: &AiStore, key: &str, word: &str) {
    store
        .record_ai_result(&NewAiResult {
            correlation: CorrelationId::new("c-old".to_owned()),
            message_account: "work".to_owned(),
            message_key: key.to_owned(),
            stage: "categorise".to_owned(),
            rule_id: None,
            verdict: "ok".to_owned(),
            category: Some(word.to_owned()),
        })
        .expect("the categorise row lands");
}

/// An app over `provider` with the AI bundle wired over `store`, synced
/// once so the engine holds the mail.
async fn docs_app(
    provider: fakes::FakeProvider,
    complete: fakes::ai::FixtureComplete,
    store: Arc<AiStore>,
) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        fakes::ai::FixtureJudge::answering(yes()),
        complete,
        store,
        astra_ai::DlpPolicy::Allow,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// Seven primary messages, `m1` the oldest at 08:00 through `m7` the
/// newest at 14:00, each with its plain source, all categorised primary
/// in the store.
async fn seven_primary() -> (
    App<fakes::FakeProvider>,
    Arc<AiStore>,
    fakes::ai::FixtureComplete,
) {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for at in 1..=7 {
        let key = format!("m{at}");
        messages.push(inbox_mail(&key, &format!("Note {at}"), 7 + at));
        sources.push(source(&key));
    }
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    for at in 1..=7 {
        categorised(&store, &format!("m{at}"), "primary");
    }
    let complete = fakes::ai::FixtureComplete::answering(DIGESTED);
    let probe = complete.clone();
    let app = docs_app(
        fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources),
        complete,
        Arc::clone(&store),
    )
    .await;
    (app, store, probe)
}

/// The bounds quantise to whole UTC days ending on the "now" day: the
/// ceiling of the days clamp, the floor of one, and the period pair the
/// store keys its replace on.
#[test]
fn the_window_bounds_quantise_to_whole_utc_days_behind_the_clamp() {
    let now = at(12, 34);
    let day = at(0, 0);
    assert_eq!(
        window_bounds(1, now),
        (day, day_end()),
        "a one-day window covers today whole, whatever the hour asked at",
    );
    assert_eq!(
        window_bounds(7, now),
        (day - 6 * 86_400, day_end()),
        "a seven-day window reaches six midnights back through today",
    );
    assert_eq!(
        window_bounds(0, now),
        (day, day_end()),
        "a day under the floor clamps up to one",
    );
    assert_eq!(
        window_bounds(4_000, now),
        (day - 30 * 86_400, day_end()),
        "a day over the ceiling clamps down to the pinned maximum",
    );
    assert_eq!(DOCUMENT_WINDOW_DEFAULT_DAYS, 7);
    assert_eq!(DOCUMENT_WINDOW_MAX_DAYS, 31);
    assert_eq!(DOCUMENT_WINDOW_READ_LIMIT, 200);
}

/// The collector feeds the window newest first: the head
/// [`DOCUMENT_HEAD_LIMIT`] the prompt rides per category are the newest
/// messages of the bucket, the older ones omitted while the rollup
/// still counts every one.
#[tokio::test]
async fn the_collector_feeds_the_window_newest_first_for_the_head_five() {
    let (app, _store, probe) = seven_primary().await;

    let view = app
        .generate_document(&account(), 1, AiDocumentKind::Digest, day_end())
        .await
        .expect("the generation answers");
    assert_eq!(
        view.content, "Two replies wait; the newsletter can wait.",
        "the view carries the parsed digest",
    );
    assert_eq!(probe.call_count(), 1, "the digest is one call");
    let call = &probe.calls()[0];
    assert_eq!(call.use_case, astra_ai::LlmUseCase::Digest);
    assert!(
        !call.correlation.as_str().is_empty(),
        "the pull minted the generation turn's correlation",
    );
    assert_eq!(DOCUMENT_HEAD_LIMIT, 5);

    let prompt = &call.prompt;
    assert!(
        prompt.contains("primary: 7"),
        "the rollup counts every window message:\n{prompt}",
    );
    assert!(
        prompt.contains("(7 message(s), the newest 5 shown)"),
        "the category line names the true count beside the head count:\n{prompt}",
    );
    let mut head_at = Vec::new();
    for at in [7, 6, 5, 4, 3] {
        let found = prompt
            .find(&format!("Note {at}"))
            .unwrap_or_else(|| panic!("the newest five ride the prompt:\n{prompt}"));
        head_at.push(found);
    }
    assert!(
        head_at.windows(2).all(|pair| pair[0] < pair[1]),
        "the head five ride newest first, the newest of the bucket leading:\n{prompt}",
    );
    for old in ["Note 2", "Note 1"] {
        assert!(
            !prompt.contains(old),
            "the older messages ride nothing past the head limit:\n{prompt}",
        );
    }
}

/// A message whose body cannot be fetched rides nothing at all: the
/// rollup counts the messages that resolved, never the row the reading
/// path could not open.
#[tokio::test]
async fn a_body_that_cannot_be_fetched_skips_the_message_whole() {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for at in 1..=3 {
        let key = format!("m{at}");
        messages.push(inbox_mail(&key, &format!("Note {at}"), 8 + at));
        if at != 2 {
            sources.push(source(&key));
        }
    }
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    for at in 1..=3 {
        categorised(&store, &format!("m{at}"), "primary");
    }
    let complete = fakes::ai::FixtureComplete::answering(DIGESTED);
    let probe = complete.clone();
    let app = docs_app(
        fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources),
        complete,
        Arc::clone(&store),
    )
    .await;

    let view = app
        .generate_document(&account(), 1, AiDocumentKind::Digest, day_end())
        .await
        .expect("the generation answers");
    assert_eq!(view.content, "Two replies wait; the newsletter can wait.");
    let prompt = &probe.calls()[0].prompt;
    assert!(
        prompt.contains("primary: 2"),
        "only the resolved messages roll up:\n{prompt}",
    );
    assert!(
        !prompt.contains("Note 2"),
        "the sourceless message rides nothing:\n{prompt}",
    );
}

/// The same period generated again replaces the stored row and names it
/// under the regenerated flag: one row per kind per period, its content
/// the newest generation's.
#[tokio::test]
async fn the_same_period_regenerated_replaces_the_row_under_the_flag() {
    let complete = fakes::ai::FixtureComplete::scripted(&[DIGESTED, REDIGESTED]);
    let probe = complete.clone();
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for at in 1..=7 {
        let key = format!("m{at}");
        messages.push(inbox_mail(&key, &format!("Note {at}"), 7 + at));
        sources.push(source(&key));
    }
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    for at in 1..=7 {
        categorised(&store, &format!("m{at}"), "primary");
    }
    let app = docs_app(
        fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources),
        complete,
        Arc::clone(&store),
    )
    .await;

    let first = app
        .generate_document(&account(), 1, AiDocumentKind::Digest, day_end())
        .await
        .expect("the first generation answers");
    assert_eq!(first.content, "Two replies wait; the newsletter can wait.");
    assert!(
        !first.regenerated,
        "the period's first generation replaces nothing"
    );

    let second = app
        .generate_document(&account(), 1, AiDocumentKind::Digest, day_end())
        .await
        .expect("the second generation answers");
    assert_eq!(second.content, "All quiet now.");
    assert!(
        second.regenerated,
        "the same period's second generation names the replace"
    );
    assert_eq!(probe.call_count(), 2, "each generation spent its own call");

    let rows = store.documents_for("work").unwrap();
    assert_eq!(rows.len(), 1, "the replace left one row behind the key");
    assert_eq!(
        rows[0].content, "All quiet now.",
        "the newest generation owns the row"
    );
}

/// The latest read answers the stored row of its kind alone: the view
/// carries the period and content the panel renders, and a kind nothing
/// generated answers null.
#[tokio::test]
async fn the_latest_read_answers_the_kind_s_own_row_or_none() {
    let (app, _store, _probe) = seven_primary().await;

    let generated = app
        .generate_document(&account(), 1, AiDocumentKind::Digest, day_end())
        .await
        .expect("the generation answers");
    let latest = app
        .latest_document(&account(), AiDocumentKind::Digest)
        .expect("the stored digest answers");
    assert_eq!(latest.period_start, generated.period_start);
    assert_eq!(latest.period_end, generated.period_end);
    assert_eq!(latest.content, generated.content);
    assert!(
        app.latest_document(&account(), AiDocumentKind::Report)
            .is_none(),
        "no report was generated, so none answers",
    );
}

/// The report rides the same collector with the Sent mailbox's rows
/// inside the window counted in code: the stats block names the sent
/// count the structured call grounds itself in.
#[tokio::test]
async fn the_report_counts_the_sent_mailbox_s_rows_inside_the_window() {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for at in 1..=2 {
        let key = format!("m{at}");
        messages.push(inbox_mail(&key, &format!("Note {at}"), 8 + at));
        sources.push(source(&key));
    }
    for (at, hour) in [(1, 10), (2, 11)] {
        let key = format!("s{at}");
        messages.push(fakes::delivered_at(
            fakes::mail(&key, "sent", &format!("Sent {at}")),
            hour,
            30,
        ));
        sources.push(source(&key));
    }
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    categorised(&store, "m1", "primary");
    categorised(&store, "m2", "newsletter");
    let complete = fakes::ai::FixtureComplete::answering(REPORTED);
    let probe = complete.clone();
    let app = docs_app(
        fakes::FakeProvider::with_sources(vec![fakes::inbox(), fakes::sent()], messages, sources),
        complete,
        Arc::clone(&store),
    )
    .await;

    let view = app
        .generate_document(&account(), 1, AiDocumentKind::Report, day_end())
        .await
        .expect("the report answers");
    assert!(
        view.content.contains("Executive summary")
            && view.content.contains("Response patterns")
            && view.content.contains("Recommendations"),
        "the view carries the assembled three-section document",
    );
    let call = &probe.calls()[0];
    assert_eq!(call.use_case, astra_ai::LlmUseCase::Report);
    assert!(
        call.prompt.contains("sent by the user in the window: 2"),
        "the sent count came from the Sent mailbox's rows in the window:\n{}",
        call.prompt,
    );
}

// The split, empty-window, no-bundle, and wire-pin tests, a child of
// this module so the fixtures above stay one compiled copy.
#[cfg(test)]
#[path = "drive_split_tests.rs"]
mod split;
