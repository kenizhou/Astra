// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the write gates beyond `may_write`: the occurrence-token gate
// that refuses a stale token before any occurrence-scoped write is issued,
//! and the refused-write row a surface sees when the engine turns a write
// away outright.

use std::sync::{Arc, Mutex};

use astra_viewmodel::CalendarWriteState;

use crate::calendar_fakes::{account, calendar_app, utc, weekly_event, CalendarFake};
use crate::{
    reference::EventRef, CalendarIntent, EventDrag, EventEdge, EventEdit, Intent, Surface,
};

/// The fortnight the fixtures live in, as the range every test sets first.
fn week() -> Intent {
    Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
    })
}

/// The standup's event reference, the way a row names one.
fn standup() -> EventRef {
    EventRef::from_parts("work", "evt-1".to_owned()).unwrap()
}

/// Boots the app over the weekly standup, synced, the prefix every gate test
/// reads.
async fn synced(
    fake: CalendarFake,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> crate::App<CalendarFake> {
    let app = calendar_app(vec![account("work", fake)], surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app
}

#[tokio::test]
async fn a_stale_occurrence_token_refuses_and_a_fresh_one_passes() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let deletes = Arc::clone(&fake.deletes);
    let app = synced(fake, &surfaces).await;

    // A token the Monday rule never produces: a Wednesday the grid never
    // drew. The mapping alone would render it happily (it is a real wall
    // clock), which is exactly why the store re-mints and compares.
    let stale = "2026-10-28T09:00:00".parse().unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
        event: standup(),
        occurrence: Some(stale),
    }))
    .await
    .unwrap();
    assert!(
        deletes.lock().unwrap().is_empty(),
        "a stale token writes nothing: the second override would split at a \
         time the rule never produces"
    );

    // A fresh token, the one the snapshot's own second row carries: the write
    // goes through and targets the one occurrence.
    let fresh = app
        .calendar()
        .occurrences
        .iter()
        .filter(|row| row.event.as_str() == "evt-1" && row.occurrence_start.is_some())
        .nth(1)
        .and_then(|row| row.occurrence_start)
        .expect("the series rows carry their tokens");
    app.dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
        event: standup(),
        occurrence: Some(fresh),
    }))
    .await
    .unwrap();
    assert_eq!(deletes.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_stale_token_refuses_the_occurrence_edit_and_move_the_same_way() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let patches = Arc::clone(&fake.patches);
    let app = synced(fake, &surfaces).await;
    let stale = "2026-10-28T09:00:00".parse().unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
        event: standup(),
        edit: EventEdit {
            title: Some("Renamed".to_owned()),
            start: Some(stale),
            end: Some("2026-10-28T09:30:00".parse().unwrap()),
            occurrence: Some(stale),
            ..EventEdit::default()
        },
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::MoveEvent {
        event: standup(),
        drag: EventDrag {
            edge: EventEdge::Whole,
            days: 1,
            minutes: 0,
            occurrence: Some(stale),
        },
    }))
    .await
    .unwrap();

    assert!(
        patches.lock().unwrap().is_empty(),
        "the token gate stands before both occurrence-scoped writes, not only \
         the delete"
    );
}

#[tokio::test]
async fn a_refused_write_surfaces_the_failed_with_retry_row() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    *fake.fail_writes.lock().unwrap() = 1;
    let patches = Arc::clone(&fake.patches);
    let app = synced(fake, &surfaces).await;

    app.dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
        event: standup(),
        edit: EventEdit {
            title: Some("Renamed".to_owned()),
            ..EventEdit::default()
        },
    }))
    .await
    .unwrap();

    assert!(
        patches.lock().unwrap().is_empty(),
        "the engine refused the write; nothing crossed"
    );
    let states: Vec<_> = app
        .calendar()
        .write_states
        .iter()
        .map(|row| row.state)
        .collect();
    assert_eq!(
        states,
        [CalendarWriteState::FailedWithRetry],
        "a write that silently went nowhere is indistinguishable on screen \
         from one that was never asked for"
    );

    // The re-read a refresh performs resolves the row, and shows the change
    // is not there to keep.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(app.calendar().write_states.is_empty());
    assert_eq!(
        app.calendar()
            .occurrences
            .iter()
            .find(|row| row.event.as_str() == "evt-1")
            .map(|row| row.title.as_str()),
        Some("Standup"),
        "the refused rename never landed"
    );
}
