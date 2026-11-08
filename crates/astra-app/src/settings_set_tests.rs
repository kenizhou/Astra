// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the settings set arms: every total field persists into the
//! preferences document before the surface answers, the zone and horizon
//! refusals are typed on the snapshot, and the view mode publishes both the
//! settings surface and the mailbox list, whichever intent moved it.

use std::sync::{Arc, Mutex};

use astra_viewmodel::SettingsError;

use crate::calendar_fakes::CalendarFake;
use crate::preferences::load_preferences;
use crate::protocol::{SettingsIntent, ThemeMode, ViewMode, WeekdayWire};
use crate::reference::CalendarRef;
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{App, Intent, Surface};

/// One mail-half account named `work`, the shape every set test starts from.
fn app(prefs: Option<std::path::PathBuf>) -> App<CalendarFake> {
    let surfaces = recorded();
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    settings_app(
        vec![crate::settings_fakes::account("work", vec![mail_half()])],
        ops,
        prefs,
        &surfaces,
    )
}

/// The shared surface recorder, so the builders stay one-liners.
fn recorded() -> Arc<Mutex<Vec<Surface>>> {
    Arc::new(Mutex::new(Vec::new()))
}

/// A scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Dispatches `intent` and asserts it landed.
async fn lands(app: &crate::App<CalendarFake>, intent: SettingsIntent) {
    app.dispatch(Intent::Settings(intent)).await.unwrap();
}

#[tokio::test]
async fn every_total_set_arm_persists_and_republishes_the_block() {
    let dir = scratch("total-arms");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::SetTheme {
            mode: ThemeMode::Dark,
        },
    )
    .await;
    lands(&app, SettingsIntent::SetClock { twenty_four: true }).await;
    lands(
        &app,
        SettingsIntent::SetZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    lands(
        &app,
        SettingsIntent::SetFirstDayOfWeek {
            weekday: WeekdayWire::Su,
        },
    )
    .await;
    lands(&app, SettingsIntent::SetHorizon { days: 31 }).await;
    lands(&app, SettingsIntent::SetQuoteStyle { below: false }).await;
    lands(
        &app,
        SettingsIntent::SetDefaultCalendar {
            calendar: Some(CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap()),
        },
    )
    .await;
    lands(
        &app,
        SettingsIntent::SetNotificationsEnabled { enabled: false },
    )
    .await;

    let snapshot = app.settings();
    assert_eq!(snapshot.settings.theme, ThemeMode::Dark);
    assert!(snapshot.settings.twenty_four_clock);
    assert_eq!(snapshot.settings.zone, "Europe/Berlin");
    assert_eq!(snapshot.settings.first_day_of_week, WeekdayWire::Su);
    assert_eq!(snapshot.settings.horizon_days, 31);
    assert!(!snapshot.settings.quote_below);
    assert!(!snapshot.settings.notifications_enabled);
    assert_eq!(
        snapshot
            .settings
            .default_calendar
            .map(|pin| pin.key.key().as_str().to_owned()),
        Some("cal-1".to_owned())
    );
    assert_eq!(snapshot.error, None, "every write landed");

    // The block on screen is the block on disk: a relaunch over the same
    // file holds every choice.
    let persisted = load_preferences(&path);
    assert_eq!(persisted.theme, ThemeMode::Dark);
    assert!(persisted.twenty_four_clock);
    assert_eq!(persisted.zone, "Europe/Berlin");
    assert_eq!(persisted.first_day_of_week, WeekdayWire::Su);
    assert_eq!(persisted.horizon_days, 31);
    assert!(!persisted.quote_below);
    assert!(!persisted.notifications_enabled);
    assert_eq!(
        persisted
            .default_calendar
            .map(|pin| pin.key.key().as_str().to_owned()),
        Some("cal-1".to_owned())
    );
}

#[tokio::test]
async fn an_unresolvable_zone_is_refused_typed_and_the_old_zone_stands() {
    let dir = scratch("zone-refusal");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::SetZone {
            zone: "Mars/Olympus".to_owned(),
        },
    )
    .await;

    let snapshot = app.settings();
    assert_eq!(
        snapshot.error,
        Some(SettingsError::ZoneUnresolvable),
        "the refusal is typed on the snapshot"
    );
    assert_eq!(
        snapshot.settings.zone, "Etc/UTC",
        "the refused zone never moved the standing one"
    );
    assert_eq!(
        load_preferences(&path).zone,
        "Etc/UTC",
        "nothing reached the file"
    );

    // The next write that lands clears the refusal: the channel states the
    // newest write, not the history.
    lands(
        &app,
        SettingsIntent::SetZone {
            zone: "Europe/Amsterdam".to_owned(),
        },
    )
    .await;
    assert_eq!(app.settings().error, None);
    assert_eq!(app.settings().settings.zone, "Europe/Amsterdam");
}

#[tokio::test]
async fn the_horizon_refuses_outside_seven_to_three_hundred_sixty_five_days() {
    let app = app(None);
    for days in [0, 6, 366, 4000] {
        lands(&app, SettingsIntent::SetHorizon { days }).await;
        assert_eq!(
            app.settings().error,
            Some(SettingsError::HorizonOutOfBounds),
            "a horizon of {days} days is out of bounds"
        );
        assert_eq!(
            app.settings().settings.horizon_days,
            42,
            "the standing horizon never moved"
        );
    }
    for days in [7, 365] {
        lands(&app, SettingsIntent::SetHorizon { days }).await;
        assert_eq!(app.settings().error, None, "a horizon of {days} days lands");
        assert_eq!(app.settings().settings.horizon_days, days);
    }
}

#[tokio::test]
async fn the_settings_view_mode_publishes_the_settings_surface_and_the_list() {
    let surfaces = recorded();
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let app = settings_app(
        vec![crate::settings_fakes::account("work", vec![mail_half()])],
        ops,
        None,
        &surfaces,
    );
    // A folder is showing, the scope whose list snapshot carries the
    // grouping it lays rows out by.
    app.dispatch(Intent::SelectFolder {
        folder: crate::reference::FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    lands(
        &app,
        SettingsIntent::SetViewMode {
            mode: ViewMode::Threaded,
        },
    )
    .await;

    let signalled = surfaces.lock().unwrap().clone();
    assert!(
        signalled.contains(&Surface::Settings),
        "the settings surface answered: {signalled:?}"
    );
    assert!(
        signalled.contains(&Surface::MailboxList),
        "the grouping is the list's own state too: {signalled:?}"
    );
    assert_eq!(app.settings().settings.view_mode, ViewMode::Threaded);
    assert_eq!(app.mailbox_list().view_mode, ViewMode::Threaded);
    assert_eq!(
        app.settings().error,
        None,
        "a grouping change that landed answers no error"
    );
}

#[tokio::test]
async fn the_list_s_view_mode_arm_delegates_to_the_same_path() {
    let surfaces = recorded();
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let app = settings_app(
        vec![crate::settings_fakes::account("work", vec![mail_half()])],
        ops,
        Some(scratch("list-delegation").join("preferences.toml")),
        &surfaces,
    );
    app.dispatch(Intent::SelectFolder {
        folder: crate::reference::FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();

    // The list arm keeps its wire and its list republish, and answers on the
    // settings surface beside them: one path serves both intents.
    assert_eq!(app.mailbox_list().view_mode, ViewMode::Threaded);
    assert_eq!(app.settings().settings.view_mode, ViewMode::Threaded);
    assert_eq!(
        app.settings().error,
        None,
        "the shared path answers the persist's failure, not the fact it changed"
    );
    let signalled = surfaces.lock().unwrap().clone();
    assert!(signalled.contains(&Surface::MailboxList));
    assert!(signalled.contains(&Surface::Settings));
}

#[tokio::test]
async fn refresh_rebuilds_and_republishes_the_snapshot() {
    let surfaces = recorded();
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let app = settings_app(
        vec![crate::settings_fakes::account("work", vec![mail_half()])],
        ops,
        None,
        &surfaces,
    );

    lands(&app, SettingsIntent::Refresh).await;

    let snapshot = app.settings();
    assert_eq!(snapshot.settings.zone, "Etc/UTC", "the defaults stand");
    assert_eq!(snapshot.about.version, env!("CARGO_PKG_VERSION"));
    assert!(
        !snapshot.about.attributions.is_empty(),
        "the core names the stacks it is built on"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::Settings),
        "the refresh published"
    );
}
