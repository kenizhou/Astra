// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar cadence body: the periodic refresh syncs every
//! account and publishes the diary only when it moved, so the standing
//! tick signals on change alone.

use std::sync::{Arc, Mutex};

use crate::calendar_fakes::{account, calendar_app, weekly_event, CalendarFake};
use crate::Surface;

#[tokio::test]
async fn the_first_refresh_publishes_the_filled_diary() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    surfaces.lock().unwrap().clear();

    app.refresh_calendars_if_changed().await;

    assert!(
        surfaces.lock().unwrap().contains(&Surface::Calendar),
        "a diary that filled from empty is a change the surface hears about"
    );
    assert!(
        !app.calendar().occurrences.is_empty(),
        "the published snapshot holds the fetched occurrence"
    );
}

#[tokio::test]
async fn an_unchanged_refresh_publishes_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.refresh_calendars_if_changed().await;
    let published = app.calendar();
    surfaces.lock().unwrap().clear();

    app.refresh_calendars_if_changed().await;

    assert!(
        !surfaces.lock().unwrap().contains(&Surface::Calendar),
        "a tick whose sync changed nothing signals nothing"
    );
    assert_eq!(
        app.calendar(),
        published,
        "the published diary stands as it was"
    );
}

#[tokio::test]
async fn a_changed_diary_publishes_again() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(weekly_event(
        "evt-1",
        "Standup",
        "2026-10-19T09:00:00",
    ));
    let diary = Arc::clone(&fake.events);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.refresh_calendars_if_changed().await;
    let before = app.calendar().occurrences.len();
    surfaces.lock().unwrap().clear();

    diary
        .lock()
        .unwrap()
        .push(weekly_event("evt-2", "Review", "2026-10-20T10:00:00"));
    app.refresh_calendars_if_changed().await;

    assert!(
        surfaces.lock().unwrap().contains(&Surface::Calendar),
        "a diary that moved is a change the surface hears about"
    );
    assert!(
        app.calendar().occurrences.len() > before,
        "the published snapshot holds the new series' occurrences too"
    );
}
