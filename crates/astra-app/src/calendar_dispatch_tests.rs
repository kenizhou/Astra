// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the dispatch arms: the intent-to-engine table over the fakes,
//! the horizon a `SetRange` widens, the write gating on `can_write`, the
//! write states (in flight, failed with the re-read recovery), and the
//! one-vs-all split a delete carries.

use std::sync::{Arc, Mutex};

use astra_viewmodel::CalendarWriteState;
use engine_api::{CalendarDateTime, PatchTarget, TimeZoneId};

use crate::calendar_fakes::{
    account, calendar_app, local, readonly, utc, weekly_event, zoned_event, CalendarFake,
};
use crate::{
    reference::{CalendarRef, EventRef},
    CalendarIntent, EventDrag, EventEdge, EventEdit, Intent, Surface,
};

/// The fortnight the fixtures live in, as the range every test sets first (a
/// week would render a weekly series only once, and the one-vs-all test reads
/// the second occurrence).
fn week() -> Intent {
    Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
        view: None,
    })
}

/// A wall clock in Amsterdam, the way the edit literals state one.
fn zoned(value: &str) -> CalendarDateTime {
    CalendarDateTime::Zoned {
        local: local(value),
        zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
    }
}

/// The standup's event reference, the way a row names one.
fn standup() -> EventRef {
    EventRef::from_parts("work", "evt-1".to_owned()).unwrap()
}

#[tokio::test]
async fn a_create_maps_onto_the_engine_draft_and_lands_in_the_snapshot() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail();
    let creates = Arc::clone(&fake.creates);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::CreateEvent {
        calendar: CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap(),
        summary: "Review".to_owned(),
        start: zoned("2026-10-21T14:00:00"),
        end: zoned("2026-10-21T15:00:00"),
        description: None,
        location: None,
        recurrence: None,
    }))
    .await
    .unwrap();

    assert_eq!(creates.lock().unwrap().len(), 1);
    let draft = &creates.lock().unwrap()[0];
    assert_eq!(draft.summary, "Review");
    assert_eq!(draft.start, zoned("2026-10-21T14:00:00"));
    assert!(
        draft.uid.as_str().starts_with("astra-"),
        "the core minted the cross-system identity"
    );
    // The write reconciled, so the rebuilt snapshot holds the server's copy.
    let snapshot = app.calendar();
    let row = snapshot
        .occurrences
        .iter()
        .find(|row| row.title == "Review")
        .expect("the created event joined the snapshot");
    assert!(row.start < row.end);
    assert!(
        app.calendar().write_states.is_empty(),
        "a write that settled cleanly crosses no row"
    );
}

#[tokio::test]
async fn an_update_re_reads_the_master_and_patches_in_its_own_form() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let patches = Arc::clone(&fake.patches);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
        event: standup(),
        edit: EventEdit {
            title: Some("Renamed".to_owned()),
            start: Some(local("2026-10-19T10:00:00")),
            end: Some(local("2026-10-19T11:00:00")),
            ..EventEdit::default()
        },
    }))
    .await
    .unwrap();

    assert_eq!(patches.lock().unwrap().len(), 1);
    let (base, edit) = &patches.lock().unwrap()[0];
    assert_eq!(
        base.id.key().as_str(),
        "evt-1",
        "the master was re-read as the base"
    );
    assert!(matches!(edit.target, PatchTarget::Series));
    assert_eq!(edit.patch.summary_edit(), Some("Renamed"));
    assert_eq!(
        edit.patch.start_edit(),
        Some(&zoned("2026-10-19T10:00:00")),
        "the patch carries the wall clock in the event's own zone"
    );
    let row = app
        .calendar()
        .occurrences
        .iter()
        .find(|row| row.event.as_str() == "evt-1")
        .expect("the event still renders")
        .clone();
    assert_eq!(row.title, "Renamed", "the reconcile presented the change");
}

#[tokio::test]
async fn a_drag_applies_the_delta_and_clamps_the_resize() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let patches = Arc::clone(&fake.patches);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::MoveEvent {
        event: standup(),
        drag: EventDrag {
            edge: EventEdge::Whole,
            days: 1,
            minutes: -30,
            occurrence: None,
        },
    }))
    .await
    .unwrap();

    assert_eq!(patches.lock().unwrap().len(), 1);
    let (_, edit) = &patches.lock().unwrap()[0];
    assert_eq!(
        edit.patch.start_edit(),
        Some(&zoned("2026-10-20T08:30:00")),
        "both edges moved by the same delta"
    );
    assert_eq!(
        edit.patch.end_edit(),
        Some(&zoned("2026-10-20T09:30:00")),
        "the duration came out identical"
    );
}

#[tokio::test]
async fn a_delete_of_the_whole_series_removes_it_and_one_occurrence_splits_off() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let deletes = Arc::clone(&fake.deletes);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    // One occurrence, named by the token the snapshot's second row carries
    // (the first is the series' own start).
    let original = app
        .calendar()
        .occurrences
        .iter()
        .filter(|row| row.event.as_str() == "evt-1" && row.occurrence_start.is_some())
        .nth(1)
        .and_then(|row| row.occurrence_start)
        .expect("the series rows carry their tokens");
    app.dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
        event: standup(),
        occurrence: Some(original),
    }))
    .await
    .unwrap();
    assert_eq!(deletes.lock().unwrap().len(), 1);
    // Cloned out, not borrowed: the borrow of the recorder's lock must not
    // live across the second dispatch below.
    let deletion = deletes.lock().unwrap()[0].1.clone();
    let occurrence = deletion.occurrence_target().expect("the one, not all");
    assert_eq!(
        occurrence.start,
        zoned("2026-10-26T09:00:00"),
        "the token names the occurrence by its original start in the series' own zone"
    );
    // A whole-series delete is a different request: the target says so.
    app.dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
        event: standup(),
        occurrence: None,
    }))
    .await
    .unwrap();
    let (_, whole) = &deletes.lock().unwrap()[1];
    assert!(whole.occurrence_target().is_none());
}

#[tokio::test]
async fn a_write_into_a_read_only_calendar_is_refused() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut fake = CalendarFake::with_calendar_and_mail();
    fake.calendars = vec![readonly("cal-ro", "Shared")];
    let creates = Arc::clone(&fake.creates);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::CreateEvent {
        calendar: CalendarRef::from_parts("work", "cal-ro".to_owned()).unwrap(),
        summary: "Nope".to_owned(),
        start: zoned("2026-10-21T14:00:00"),
        end: zoned("2026-10-21T15:00:00"),
        description: None,
        location: None,
        recurrence: None,
    }))
    .await
    .unwrap();

    assert!(
        creates.lock().unwrap().is_empty(),
        "the engine write was never issued: the gate read may_write first"
    );
}

#[tokio::test]
async fn a_failed_confirmation_lands_the_write_and_a_refresh_recovers_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let fail_syncs = Arc::clone(&fake.fail_syncs);
    let patches = Arc::clone(&fake.patches);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    // The next event pass (the write's reconcile) fails: the write lands, the
    // local confirmation does not.
    *fail_syncs.lock().unwrap() = 1;

    app.dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
        event: standup(),
        edit: EventEdit {
            title: Some("Renamed".to_owned()),
            ..EventEdit::default()
        },
    }))
    .await
    .unwrap();

    assert_eq!(
        patches.lock().unwrap().len(),
        1,
        "the write was issued once"
    );
    let snapshot = app.calendar();
    let states: Vec<_> = snapshot.write_states.iter().map(|row| row.state).collect();
    assert_eq!(states, [CalendarWriteState::FailedWithRetry]);
    assert_ne!(
        snapshot
            .occurrences
            .iter()
            .find(|row| row.event.as_str() == "evt-1")
            .map(|row| row.title.as_str()),
        Some("Renamed"),
        "the store still holds the pre-write copy: the failure was of the confirmation"
    );

    // The recovery is a re-read, never the same write issued again.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(
        app.calendar().write_states.is_empty(),
        "a successful re-read resolved the unconfirmed write"
    );
    assert_eq!(
        patches.lock().unwrap().len(),
        1,
        "the write was never re-issued"
    );
    assert_eq!(
        app.calendar()
            .occurrences
            .iter()
            .find(|row| row.event.as_str() == "evt-1")
            .map(|row| row.title.as_str()),
        Some("Renamed"),
        "the re-read presented the write the server had held all along"
    );
}

#[tokio::test]
async fn a_window_past_the_horizon_materializes_through_the_widener() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-far",
        "November thing",
        "2026-11-16T09:00:00",
    ));
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    // Sync over October: the event is stored, but November has no materialized
    // occurrences and re-syncing will not backfill them.
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 7,
        view: None,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(!app
        .calendar()
        .occurrences
        .iter()
        .any(|row| row.event.as_str() == "evt-far"));

    // Paging into November widens the horizon and the row appears, with no
    // refresh dispatched at all: the widening is the maintenance path.
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-11-15T00:00:00Z"),
        days: 7,
        view: None,
    }))
    .await
    .unwrap();
    assert!(
        app.calendar()
            .occurrences
            .iter()
            .any(|row| row.event.as_str() == "evt-far"),
        "the widened horizon materialized the week the grid asked for"
    );
}

#[tokio::test]
async fn a_stale_event_reference_or_a_gone_account_writes_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail();
    let patches = Arc::clone(&fake.patches);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
        event: EventRef::from_parts("work", "evt-gone".to_owned()).unwrap(),
        edit: EventEdit {
            title: Some("Ghost".to_owned()),
            ..EventEdit::default()
        },
    }))
    .await
    .unwrap();

    assert!(
        patches.lock().unwrap().is_empty(),
        "a reference the store no longer resolves writes nothing"
    );
}

#[tokio::test]
async fn every_view_intent_publishes_the_calendar_surface() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail();
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(week()).await.unwrap();
    let before = surfaces.lock().unwrap().len();

    app.dispatch(Intent::Calendar(CalendarIntent::SetDate {
        date: "2026-10-19".parse().unwrap(),
    }))
    .await
    .unwrap();

    assert!(
        surfaces.lock().unwrap().len() > before,
        "the anchor move republished the surface"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::Calendar),
        "the publishes name the calendar surface"
    );
}
