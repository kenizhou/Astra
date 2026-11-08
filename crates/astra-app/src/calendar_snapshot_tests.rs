// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the snapshot rebuild: the occurrence-to-master join over the
//! fakes, the participation and can_move rules the rows carry, the declined
//! filter, and the visible-calendar set.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{OccurrenceAccuracy, Participation};
use engine_api::{CalendarId, Memberships};

use crate::calendar_fakes::{
    account, all_day_event, calendar, calendar_app, meeting, overridden_weekly_event, readonly,
    utc, weekly_event, zoned_event, CalendarFake,
};
use crate::{reference::CalendarRef, CalendarIntent, Intent, Surface};

use super::series_edit_warns;

/// The fortnight the diary fixtures live in, as the range every test sets
/// first (a test that relied on the default window would rely on the real
/// clock, and a week would render a weekly series only once).
fn week() -> Intent {
    Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
        view: None,
    })
}

/// Boots the app over `fake` and lands the window plus a refresh, the prefix
/// every join test reads.
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

/// Puts `event` in the calendar keyed `key`, the two-calendar fixtures'
/// membership rewrite.
fn in_calendar(mut event: engine_api::Event, key: &str) -> engine_api::Event {
    event.calendars = Memberships::of_one(CalendarId::try_from(key).unwrap());
    event
}

/// The one row keyed by an event id, or none: the shape every row assertion
/// reads before it looks at the row's fields.
fn row<'a>(
    snapshot: &'a astra_viewmodel::CalendarSnapshot,
    key: &str,
) -> Option<&'a astra_viewmodel::OccurrenceRow> {
    snapshot
        .occurrences
        .iter()
        .find(|row| row.event.as_str() == key)
}

#[tokio::test]
async fn the_rebuild_joins_each_occurrence_to_its_master() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .holding(zoned_event("evt-1", "Standup", "2026-10-19T09:00:00"))
        .holding(weekly_event("evt-2", "Planning", "2026-10-20T11:00:00"))
        .holding(all_day_event("evt-3", "Away day", "2026-10-21"));
    let app = synced(fake, &surfaces).await;

    let snapshot = app.calendar();
    // The weekly series materializes an occurrence per day in the window; the
    // join carries the master's title and markers beside each row's instants.
    assert!(
        snapshot
            .occurrences
            .iter()
            .filter(|row| row.event.as_str() == "evt-2")
            .count()
            > 1,
        "a series renders once per occurrence, not once at its start"
    );
    let one_off = row(&snapshot, "evt-1").expect("the one-off joined");
    assert_eq!(one_off.title, "Standup");
    assert!(one_off.start < one_off.end);
    assert!(!one_off.recurring && !one_off.overridden && one_off.occurrence_start.is_none());
    assert_eq!(one_off.accuracy, OccurrenceAccuracy::Zoned);
    assert_eq!(one_off.calendar, CalendarId::try_from("cal-1").unwrap());

    let weekly = snapshot
        .occurrences
        .iter()
        .find(|row| row.event.as_str() == "evt-2")
        .expect("the series joined");
    assert!(weekly.recurring);
    assert!(
        weekly.occurrence_start.is_some(),
        "a series row carries the token a one-occurrence write hands back"
    );

    let day = row(&snapshot, "evt-3").expect("the all-day joined");
    assert!(day.all_day);
    assert_eq!(day.accuracy, OccurrenceAccuracy::AllDay);
}

#[tokio::test]
async fn the_manager_set_carries_the_rights_and_the_default() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut fake = CalendarFake::with_calendar_and_mail();
    fake.calendars.push(readonly("cal-ro", "Shared"));
    let app = synced(fake, &surfaces).await;

    let rows = &app.calendar().calendars;
    assert_eq!(rows.len(), 2);
    assert!(rows[0].can_write && rows[0].is_default && rows[0].visible);
    assert!(
        !rows[1].can_write,
        "the read-only calendar hides its edit affordances"
    );
    assert!(rows[1].visible, "visible by default: no set has spoken yet");
}

#[tokio::test]
async fn participation_and_can_move_follow_the_organiser_owns_rule() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut invited = meeting("evt-m", "info@test.local");
    // The meeting lives this week, where the window looks.
    invited.start = engine_api::CalendarDateTime::Zoned {
        local: "2026-10-20T16:30:00".parse().unwrap(),
        zone: engine_api::TimeZoneId::iana("Europe/Berlin").unwrap(),
    };
    let fake = CalendarFake::with_calendar_and_mail()
        .holding(zoned_event("evt-own", "Dentist", "2026-10-19T09:00:00"))
        .holding(invited);
    let app = synced(fake, &surfaces).await;

    let snapshot = app.calendar();
    let own = row(&snapshot, "evt-own").expect("the appointment joined");
    assert_eq!(own.participation, Participation::Accepted);
    assert!(
        own.can_move,
        "the user's own appointment is theirs to reshape"
    );

    let meeting_row = row(&snapshot, "evt-m").expect("the meeting joined");
    assert_eq!(
        meeting_row.participation,
        Participation::NeedsAction,
        "an unanswered invitation is the hold a grid draws dotted"
    );
    assert!(
        !meeting_row.can_move,
        "a meeting somebody else called is not silently re-timed, writable calendar or not"
    );
}

#[tokio::test]
async fn a_declined_meeting_leaves_the_diary() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut declined = meeting("evt-m", "info@test.local");
    declined.start = engine_api::CalendarDateTime::Zoned {
        local: "2026-10-20T16:30:00".parse().unwrap(),
        zone: engine_api::TimeZoneId::iana("Europe/Berlin").unwrap(),
    };
    declined.participants[1].participation_status = engine_api::ParticipationStatus::Declined;
    let fake = CalendarFake::with_calendar_and_mail().holding(declined);
    let app = synced(fake, &surfaces).await;

    assert!(
        row(&app.calendar(), "evt-m").is_none(),
        "the user said no; the diary never shows it again"
    );
}

#[tokio::test]
async fn the_visible_set_hides_and_shows_whole_calendars() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut fake = CalendarFake::with_calendar_and_mail();
    fake.calendars.push(calendar("cal-2", "Home"));
    let fake = fake
        .holding(zoned_event("evt-1", "Work stuff", "2026-10-19T09:00:00"))
        .holding(in_calendar(
            zoned_event("evt-2", "Home stuff", "2026-10-19T15:00:00"),
            "cal-2",
        ));
    let app = synced(fake, &surfaces).await;
    assert_eq!(app.calendar().occurrences.len(), 2, "everything by default");

    app.dispatch(Intent::Calendar(CalendarIntent::SetVisibleCalendars {
        ids: vec![CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap()],
    }))
    .await
    .unwrap();
    let snapshot = app.calendar();
    assert!(snapshot
        .occurrences
        .iter()
        .all(|row| row.calendar == CalendarId::try_from("cal-1").unwrap()));
    assert!(snapshot.calendars[0].visible);
    assert!(!snapshot.calendars[1].visible);

    // An empty set hides every calendar's events: the state the manager's
    // "hide all" produces, distinct from no set having spoken.
    app.dispatch(Intent::Calendar(CalendarIntent::SetVisibleCalendars {
        ids: Vec::new(),
    }))
    .await
    .unwrap();
    assert!(app.calendar().occurrences.is_empty());
}

#[tokio::test]
async fn the_now_line_rides_every_rebuild() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(CalendarFake::with_calendar_and_mail(), &surfaces).await;

    let now = app.calendar().now_data.expect("the now line crossed");
    assert_eq!(
        now.zone, "Etc/UTC",
        "the display zone until a zone intent exists"
    );
}

#[tokio::test]
async fn a_series_that_carries_overrides_warns_where_the_server_discards_them() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .discarding_overrides()
        .holding(weekly_event("evt-clean", "Standup", "2026-10-19T09:00:00"))
        .holding(overridden_weekly_event(
            "evt-touched",
            "Planning",
            "2026-10-20T11:00:00",
        ));
    let app = synced(fake, &surfaces).await;

    let snapshot = app.calendar();
    // The clean series: the user has nothing to lose, so nothing warns. A
    // warning that cannot matter is what teaches people to click past the
    // one that does.
    assert!(
        snapshot
            .occurrences
            .iter()
            .filter(|row| row.event.as_str() == "evt-clean")
            .all(|row| !row.series_edit_warns),
        "a clean series never warns"
    );
    // The series the user changed one occurrence of: every row of it carries
    // the warning, because every row of it opens the same series editor.
    let touched: Vec<_> = snapshot
        .occurrences
        .iter()
        .filter(|row| row.event.as_str() == "evt-touched")
        .collect();
    assert!(!touched.is_empty(), "the touched series joined");
    assert!(
        touched.iter().all(|row| row.series_edit_warns),
        "every occurrence of the override-carrying series warns"
    );
    // The two facts are ANDed: no policy fact, no warning (and no write
    // either, which the may_write gate already refuses).
    assert!(!series_edit_warns(None, true));
    assert!(!series_edit_warns(
        Some(engine_api::OverrideSurvival::kept()),
        true
    ));
    assert!(!series_edit_warns(
        Some(engine_api::OverrideSurvival {
            survives_time_change: false,
            survives_rule_change: false,
            clobbers_own_fields: true,
        }),
        false
    ));
}

#[tokio::test]
async fn a_server_that_keeps_overrides_never_warns() {
    // The default fake is a CalDAV-shaped transport: overrides survive a
    // series edit by construction, so the very same override-carrying series
    // that warns elsewhere carries no warning here.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(overridden_weekly_event(
        "evt-touched",
        "Planning",
        "2026-10-20T11:00:00",
    ));
    let app = synced(fake, &surfaces).await;

    assert!(
        app.calendar()
            .occurrences
            .iter()
            .filter(|row| row.event.as_str() == "evt-touched")
            .all(|row| !row.series_edit_warns),
        "the server keeps the user's overrides; there is nothing to say"
    );
}
