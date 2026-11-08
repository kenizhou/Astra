// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the brief pull's horizon widening: the shared watermark may
//! only claim a span every account's store expanded to, a refused widening
//! is a day the brief cannot see (the read failure, never a confident
//! no-meetings view), and a day beyond the dispatched range widens before
//! the pull re-reads it. A child of the brief tests, reusing their
//! fixtures; its own file to keep each test module under the 500-line
//! limit.

use std::sync::{Arc, Mutex};

use engine_api::{CalendarDate, Horizon, TimeZoneId};

use crate::calendar::{BriefDocumentError, MeetingBriefState};
use crate::calendar_fakes::{account, utc, zoned_event, CalendarFake, EventGate};
use crate::dispatch_fakes::ai::FixtureComplete;
use crate::{CalendarIntent, Intent};

use super::{account_id, diary_app, two_event_day, GOOD, GOOD_TEXT};

/// The account whose far-day diary the widening tests hold on the second
/// account, so a shared watermark cannot hide behind a single store.
fn home_id() -> engine_api::AccountId {
    engine_api::AccountId::try_from("home").unwrap()
}

/// The far day every widening test briefs: 15 December, beyond the 2
/// December high the boot's watermark stands at.
fn far_day() -> CalendarDate {
    CalendarDate::new(2026, 12, 15).unwrap()
}

/// The home diary holding one far meeting, so a widening that skips the
/// account leaves its store without the day's occurrences.
fn home_with_far_meeting() -> CalendarFake {
    CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-far-home",
        "Home far meeting",
        "2026-12-15T09:00:00",
    ))
}

/// Whether the calendar snapshot renders home's far meeting.
fn home_far_meeting_renders(app: &crate::App<CalendarFake>) -> bool {
    app.calendar()
        .occurrences
        .iter()
        .any(|row| row.account.as_str() == "home" && row.event.as_str() == "evt-far-home")
}

#[tokio::test]
async fn a_brief_beyond_the_watermark_widens_every_account_store() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&[]);
    let app = diary_app(
        vec![
            account("work", two_event_day()),
            account("home", home_with_far_meeting()),
        ],
        complete,
        &surfaces,
    )
    .await;

    // Work's brief for the far day: work holds nothing on it, and the
    // widening the brief runs may not mark the span covered for a store it
    // never expanded.
    let view = app
        .generate_meeting_brief(&account_id(), far_day())
        .await
        .expect("the empty far day answers, it does not fail");
    assert_eq!(view.state, MeetingBriefState::NoMeetings);

    // A window inside the span the brief widened and inside the watermark it
    // left: home's far meeting must render, never answer confidently empty.
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-12-10T00:00:00Z"),
        days: 6,
        view: None,
    }))
    .await
    .unwrap();
    assert!(
        home_far_meeting_renders(&app),
        "the watermark may only claim a span every account's store expanded to"
    );
}

#[tokio::test]
async fn a_refused_widening_fails_the_brief_and_rolls_the_watermark_back() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&[]);
    let gate = EventGate::new();
    let mut home = home_with_far_meeting();
    home.event_gate = Some(Arc::clone(&gate));
    let app = diary_app(
        vec![account("work", two_event_day()), account("home", home)],
        complete,
        &surfaces,
    )
    .await;

    // Hold home's event sync: the account's scope stays leased, so home's
    // expansion answers Busy for as long as the gate stands closed.
    gate.close();
    let engine = Arc::clone(&app.engine);
    let handle = app.account_handle(&home_id()).await.expect("home rides");
    let home_id = home_id();
    let sync = tokio::spawn(async move {
        let provider = &handle.providers[0];
        let horizon =
            Horizon::new(utc("2026-10-18T00:00:00Z"), utc("2026-12-02T00:00:00Z")).unwrap();
        let _ = engine
            .sync_calendar(provider, &home_id, horizon, &TimeZoneId::utc())
            .await;
    });
    gate.until_held().await;

    // One store's refusal is a day the brief cannot see: the pull answers
    // the read failure, never a confident no-meetings view.
    let failure = app
        .generate_meeting_brief(&account_id(), far_day())
        .await
        .expect_err("the refused widening is a typed failure");
    assert!(
        matches!(failure, BriefDocumentError::Occurrence(_)),
        "the failure is the read failure: {failure}"
    );

    // The watermark rolled back with the refusal: once the scope is free,
    // the same window retries its widening and home's far meeting lands.
    gate.release();
    sync.await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-12-14T00:00:00Z"),
        days: 2,
        view: None,
    }))
    .await
    .unwrap();
    assert!(
        home_far_meeting_renders(&app),
        "a rolled-back watermark lets the next window retry the widening"
    );
}

#[tokio::test]
async fn a_day_beyond_the_dispatched_range_widens_then_rereads_its_meetings() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&[GOOD]);
    let fake = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-dec",
        "December thing",
        "2026-12-15T09:00:00",
    ));
    let app = diary_app(vec![account("work", fake)], complete.clone(), &surfaces).await;

    // The dispatched fortnight ends 1 November and the boot's watermark 2
    // December: the brief day sits beyond both, so the pull widens the
    // horizon before it re-reads the day.
    let view = app
        .generate_meeting_brief(&account_id(), far_day())
        .await
        .expect("the far day briefs");
    assert_eq!(view.state, MeetingBriefState::Generated);
    assert_eq!(view.brief, GOOD_TEXT);
    assert_eq!(complete.call_count(), 1, "the far day costs its one call");
    assert_eq!(view.meetings.len(), 1);
    assert_eq!(view.meetings[0].title, "December thing");
    assert_eq!(view.meetings[0].start, utc("2026-12-15T08:00:00Z"));
}
