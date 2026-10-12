// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar's consumption of the settings: the display zone
//! the rebuild resolves floating and all-day rows through, the horizon that
//! sizes the cold window, the first day a created rule's WKST follows, and
//! the default calendar the settings block carries for the create dialog.

use std::sync::{Arc, Mutex};

use engine_api::{to_local, CalendarDateTime, Frequency, TimeZoneId};

use crate::calendar_fakes::{local, utc, weekly_event, zoned_event, CalendarFake};
use crate::protocol::{CalendarIntent, RepeatEnd, RepeatRule, SettingsIntent, WeekdayWire};
use crate::reference::CalendarRef;
use crate::settings_fakes::{settings_app, FakeAccountOps};
use crate::{App, Intent, Surface};

/// The fortnight the fixtures live in, as the range the zone test sets first
/// so the window is pinned by the intent and not the wall clock.
fn fortnight() -> Intent {
    Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
    })
}

/// A recurring floating event: the diary shape the display zone resolves.
fn floating_weekly() -> CalendarFake {
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    event.start = CalendarDateTime::Floating(local("2026-10-19T09:00:00"));
    CalendarFake::with_calendar_and_mail().holding(event)
}

/// One account named `work` over `fake`, with the scripted store installed
/// and the signalled surfaces recorded beside the app.
fn over(
    fake: CalendarFake,
    prefs: Option<std::path::PathBuf>,
) -> (App<CalendarFake>, Arc<Mutex<Vec<Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let app = settings_app(
        vec![crate::settings_fakes::account("work", vec![fake])],
        ops,
        prefs,
        &surfaces,
    );
    (app, surfaces)
}

/// A scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn a_set_zone_flows_into_the_now_line_and_the_floating_resolution() {
    let (app, surfaces) = over(floating_weekly(), None);
    app.dispatch(fortnight()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    // Before the setting, the zone is the default: the now line names UTC.
    assert_eq!(
        app.calendar().now_data.as_ref().unwrap().zone,
        "Etc/UTC",
        "the shipped default zone stands until a setting lands"
    );

    app.dispatch(Intent::Settings(SettingsIntent::SetZone {
        zone: "Europe/Berlin".to_owned(),
    }))
    .await
    .unwrap();
    // A sync after the change re-materialises the floating occurrences under
    // the new zone, the same pass a refresh would run.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    let after = app.calendar();
    assert_eq!(
        after.now_data.as_ref().unwrap().zone,
        "Europe/Berlin",
        "the now line names the persisted zone"
    );
    let berlin = TimeZoneId::iana("Europe/Berlin").unwrap();
    let row = after
        .occurrences
        .iter()
        .find(|row| row.title == "Standup")
        .expect("the floating series is on screen");
    assert_eq!(
        to_local(row.start, &berlin).unwrap(),
        local("2026-10-19T09:00:00"),
        "the floating start resolves to the Berlin wall clock"
    );
    assert_eq!(
        row.occurrence_start,
        Some(local("2026-10-19T09:00:00")),
        "the occurrence token is the event's own terms in the display zone"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::Calendar),
        "the zone change republished the calendar surface"
    );
}

/// Whether the horizon test's far day is on screen after a refresh, the
/// read every horizon assertion shares.
async fn far_day_showing(app: &App<CalendarFake>) -> bool {
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app.calendar()
        .occurrences
        .iter()
        .any(|row| row.title == "Far out")
}

#[tokio::test]
async fn the_horizon_preference_sizes_the_cold_window() {
    // An event ten days out: inside the forty-two-day default, outside a
    // seven-day one, whatever the wall clock says the moment the test runs.
    let when = {
        let soon = time::OffsetDateTime::now_utc() + time::Duration::days(10);
        format!(
            "{:04}-{:02}-{:02}T09:00:00",
            soon.year(),
            u8::from(soon.month()),
            soon.day()
        )
    };
    let (app, _surfaces) = over(
        CalendarFake::with_calendar_and_mail().holding(zoned_event("evt-far", "Far out", &when)),
        None,
    );

    assert!(
        far_day_showing(&app).await,
        "the default forty-two-day window covers ten days out"
    );

    app.dispatch(Intent::Settings(SettingsIntent::SetHorizon { days: 7 }))
        .await
        .unwrap();
    assert!(
        !far_day_showing(&app).await,
        "a seven-day cold window covers only seven days"
    );

    app.dispatch(Intent::Settings(SettingsIntent::SetHorizon { days: 42 }))
        .await
        .unwrap();
    assert!(
        far_day_showing(&app).await,
        "widening the horizon back brings the day back"
    );
}

#[tokio::test]
async fn a_created_rule_that_names_no_weekday_carries_the_setting_s_wkst() {
    let fake = CalendarFake::with_calendar_and_mail();
    let creates = Arc::clone(&fake.creates);
    let (app, _surfaces) = over(fake, None);
    app.dispatch(fortnight()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app.dispatch(Intent::Settings(SettingsIntent::SetFirstDayOfWeek {
        weekday: WeekdayWire::Su,
    }))
    .await
    .unwrap();

    let bare_weekly = RepeatRule {
        frequency: Frequency::Weekly,
        interval: 1,
        weekdays: Vec::new(),
        end: RepeatEnd::Never,
    };
    let day_naming = RepeatRule {
        weekdays: vec![engine_api::Weekday::Tu],
        ..bare_weekly.clone()
    };
    for recurrence in [bare_weekly, day_naming] {
        app.dispatch(Intent::Calendar(CalendarIntent::CreateEvent {
            calendar: CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap(),
            summary: "Standup".to_owned(),
            start: CalendarDateTime::Floating(local("2026-10-21T14:00:00")),
            end: CalendarDateTime::Floating(local("2026-10-21T15:00:00")),
            description: None,
            location: None,
            recurrence: Some(recurrence),
        }))
        .await
        .unwrap();
    }

    let drafts = creates.lock().unwrap().clone();
    assert_eq!(drafts.len(), 2);
    assert_eq!(
        drafts[0]
            .recurrence
            .as_ref()
            .unwrap()
            .rule
            .first_day_of_week,
        engine_api::Weekday::Su,
        "a rule anchored to the week boundary starts the week the setting names"
    );
    assert_eq!(
        drafts[1]
            .recurrence
            .as_ref()
            .unwrap()
            .rule
            .first_day_of_week,
        engine_api::Weekday::Mo,
        "a rule that names its weekdays keeps the engine's default start"
    );
}

#[tokio::test]
async fn the_default_calendar_surfaces_on_the_settings_block_and_persists() {
    let path = scratch("default-cal").join("preferences.toml");
    let (app, _surfaces) = over(crate::settings_fakes::mail_half(), Some(path.clone()));
    let pin = CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap();

    app.dispatch(Intent::Settings(SettingsIntent::SetDefaultCalendar {
        calendar: Some(pin.clone()),
    }))
    .await
    .unwrap();
    let pinned = app.settings().settings.default_calendar.clone();
    assert_eq!(
        pinned
            .as_ref()
            .map(|pin| (pin.account.as_str(), pin.key.key().as_str())),
        Some(("work", "cal-1")),
        "the settings block is the one source the create dialog reads"
    );
    assert_eq!(
        crate::preferences::load_preferences(&path).default_calendar,
        Some(pin.clone()),
        "the pin is on disk before the surface answers"
    );

    app.dispatch(Intent::Settings(SettingsIntent::SetDefaultCalendar {
        calendar: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        app.settings().settings.default_calendar,
        None,
        "None unpins"
    );
    assert_eq!(
        crate::preferences::load_preferences(&path).default_calendar,
        None,
        "the unpin is on disk"
    );
}
