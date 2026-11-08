// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar's persisted shape: the view kind and the anchor
//! the navigation writes as a preference, the snapshot's publication of the
//! standing shape, and the boot seed that restores an anchor from a past
//! launch exactly while an absent or corrupt shape falls back to today.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{CalendarShape, CalendarViewWire};
use engine_api::CalendarDate;

use crate::calendar_fakes::{utc, zoned_event, CalendarFake};
use crate::settings_fakes::{account, settings_app, FakeAccountOps};
use crate::{load_preferences, CalendarIntent, Intent, Surface};

/// A scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-shape-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One account named `work` over an empty diary, with the preferences
/// persisted to `prefs` and the signalled surfaces recorded beside the app.
fn over(prefs: Option<std::path::PathBuf>) -> (crate::App<CalendarFake>, Arc<Mutex<Vec<Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let app = settings_app(
        vec![account(
            "work",
            vec![CalendarFake::with_calendar_and_mail()],
        )],
        ops,
        prefs,
        &surfaces,
    );
    (app, surfaces)
}

#[tokio::test]
async fn navigation_writes_the_shape_and_the_snapshot_publishes_it() {
    let dir = scratch("navigation");
    let path = dir.join("preferences.toml");
    let (app, _surfaces) = over(Some(path.clone()));

    app.dispatch(Intent::Calendar(CalendarIntent::SetDate {
        date: CalendarDate::new(2026, 10, 6).unwrap(),
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-05T22:00:00Z"),
        days: 1,
        view: Some(CalendarViewWire::Day),
    }))
    .await
    .unwrap();

    let shape = app
        .calendar()
        .shape
        .expect("every rebuild publishes the shape");
    assert_eq!(
        shape,
        CalendarShape {
            view: CalendarViewWire::Day,
            anchor: Some(CalendarDate::new(2026, 10, 6).unwrap()),
        }
    );
    let prefs = load_preferences(&path);
    assert_eq!(prefs.calendar_view, CalendarViewWire::Day);
    assert_eq!(
        prefs.calendar_anchor,
        Some(CalendarDate::new(2026, 10, 6).unwrap())
    );
}

#[tokio::test]
async fn the_default_shape_leaves_the_file() {
    let dir = scratch("defaults");
    let path = dir.join("preferences.toml");
    let (app, _surfaces) = over(Some(path.clone()));

    // A view the file must hold: the entry exists while the person leaves a
    // non-default view standing.
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 31,
        view: Some(CalendarViewWire::Agenda),
    }))
    .await
    .unwrap();
    let written = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        written.contains("calendar_view = \"agenda\""),
        "a non-default view writes its entry: {written}"
    );

    // The view written back to its default is the unset: the file loses the
    // key again, so the default and an explicit equal are one state. A
    // window moved without naming a view never writes one either.
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 31,
        view: Some(CalendarViewWire::Month),
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
        view: None,
    }))
    .await
    .unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        !body.contains("calendar_view") && !body.contains("calendar_anchor"),
        "the defaults leave the file, exactly as the language tag does: {body}"
    );
    let prefs = load_preferences(&path);
    assert_eq!(prefs.calendar_view, CalendarViewWire::Month);
    assert_eq!(prefs.calendar_anchor, None);
}

#[tokio::test]
async fn a_persisted_anchor_restores_the_window_exactly() {
    let dir = scratch("restore");
    let path = dir.join("preferences.toml");
    std::fs::write(
        &path,
        "calendar_view = \"agenda\"\ncalendar_anchor = \"2026-09-15\"\n",
    )
    .unwrap();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let fake = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-sept",
        "September thing",
        "2026-09-15T10:00:00",
    ));
    let app = settings_app(
        vec![account("work", vec![fake])],
        ops,
        Some(path),
        &surfaces,
    );

    // No SetRange and no SetDate: the boot seed alone decides the window, so
    // the September diary paints on the day the person left it, not today.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.calendar();
    assert!(
        snapshot
            .occurrences
            .iter()
            .any(|row| row.event.as_str() == "evt-sept"),
        "the cold window opens around the restored anchor's own week"
    );
    assert_eq!(
        snapshot.shape,
        Some(CalendarShape {
            view: CalendarViewWire::Agenda,
            anchor: Some(CalendarDate::new(2026, 9, 15).unwrap()),
        })
    );
}

#[tokio::test]
async fn an_absent_or_corrupt_shape_falls_back_to_the_month_and_today() {
    let dir = scratch("absent");
    let empty = dir.join("empty.toml");
    std::fs::write(&empty, "horizon_days = 30\n").unwrap();
    let (app, _surfaces) = over(Some(empty));
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        app.calendar().shape,
        Some(CalendarShape {
            view: CalendarViewWire::Month,
            anchor: None,
        }),
        "a file without the shape fields is the default shape, whose anchor \
         is today by behaviour"
    );

    let corrupt = dir.join("corrupt.toml");
    std::fs::write(&corrupt, "calendar_view = [[not toml").unwrap();
    let (app, _surfaces) = over(Some(corrupt));
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        app.calendar().shape,
        Some(CalendarShape {
            view: CalendarViewWire::Month,
            anchor: None,
        }),
        "a file the loader cannot parse falls back to the same defaults"
    );
}
