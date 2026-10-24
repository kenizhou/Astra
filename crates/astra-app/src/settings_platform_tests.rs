// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the platform settings: the four preferences' set arms
//! persisting and republishing through the omission discipline, and the
//! device-zone offer path (the once-per-value latch, the persisted
//! once-latch across a fresh process, the picked-zone pin, the validation
//! refusal, and the acceptance riding the zone picker's own path).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use astra_viewmodel::SettingsError;

use crate::calendar_fakes::CalendarFake;
use crate::preferences::load_preferences;
use crate::protocol::SettingsIntent;
use crate::settings_fakes::{mail_half, settings_app, FakeAccountOps};
use crate::{App, Intent, Preferences, Surface};

/// One mail-half account named `work`, the shape every platform test
/// starts from.
fn app(prefs: Option<PathBuf>) -> App<CalendarFake> {
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
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-platform-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Dispatches `intent` and asserts it landed.
async fn lands(app: &App<CalendarFake>, intent: SettingsIntent) {
    app.dispatch(Intent::Settings(intent)).await.unwrap();
}

#[tokio::test]
async fn the_platform_set_arms_persist_and_republish_the_block() {
    let dir = scratch("set-arms");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(&app, SettingsIntent::SetDebugEnabled { enabled: true }).await;
    lands(
        &app,
        SettingsIntent::SetAnalyticsConsented { consented: true },
    )
    .await;
    lands(
        &app,
        SettingsIntent::SetLanguage {
            language: "fr-FR".to_owned(),
        },
    )
    .await;

    let snapshot = app.settings();
    assert!(snapshot.settings.debug_enabled);
    assert!(snapshot.settings.analytics_consented);
    assert_eq!(snapshot.settings.language, "fr-FR");
    assert_eq!(snapshot.error, None, "every write landed");

    // The block on screen is the block on disk.
    let persisted = load_preferences(&path);
    assert!(persisted.debug_enabled);
    assert!(persisted.analytics_consented);
    assert_eq!(persisted.language, "fr-FR");

    // A value written back to its default is the unset: the file loses the
    // key, the same canonicalisation the preferences document itself
    // follows.
    lands(
        &app,
        SettingsIntent::SetLanguage {
            language: "en-GB".to_owned(),
        },
    )
    .await;
    assert_eq!(app.settings().settings.language, "en-GB");
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        !body.contains("language"),
        "the default tag leaves the file: {body}"
    );
    assert_eq!(load_preferences(&path).language, "en-GB");
}

#[tokio::test]
async fn a_reported_device_zone_offers_once_per_value_per_process() {
    let dir = scratch("offer-latch");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    // The first standing offer publishes the pending zone and latches.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        Some("Europe/Berlin".to_owned()),
        "the standing offer publishes as the pending device zone"
    );
    assert!(
        load_preferences(&path).zone_offered,
        "the once-latch persists on offer"
    );

    // The dismissal clears the pending question without accepting.
    lands(&app, SettingsIntent::DismissDeviceZone).await;
    assert_eq!(app.settings().device_zone_offer, None);

    // The dismissed value never offers again this process: the latch is
    // per zone value, so the host may re-report it freely.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        None,
        "the latched value offers no second time"
    );

    // A genuinely new value (a change the host observed) offers once.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Amsterdam".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        Some("Europe/Amsterdam".to_owned()),
        "a new device zone offers once per change"
    );
}

#[tokio::test]
async fn the_persisted_once_latch_refuses_a_fresh_process_s_boot_resolution() {
    let dir = scratch("persisted-latch");
    let path = dir.join("preferences.toml");
    // A preferences file a previous process left latched: an offer was
    // made there, so the first-run offer never returns.
    let seeded = Preferences {
        zone_offered: true,
        ..Preferences::default()
    };
    crate::preferences::save_preferences(&path, &seeded).unwrap();

    let app = app(Some(path));
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        None,
        "the boot resolution after an earlier process's offer stands refused"
    );

    // And the refused boot value counts as offered, so a host's repeated
    // boot reports never nag either.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(app.settings().device_zone_offer, None);

    // A change observed while the app runs still offers once.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Amsterdam".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        Some("Europe/Amsterdam".to_owned())
    );
}

#[tokio::test]
async fn a_picked_zone_stands_and_the_offer_is_not_even_made() {
    let dir = scratch("picked-zone");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    // The picker's own write is a pick: the marker the offer defers to.
    lands(
        &app,
        SettingsIntent::SetZone {
            zone: "Europe/Amsterdam".to_owned(),
        },
    )
    .await;
    assert_eq!(app.settings().settings.zone, "Europe/Amsterdam");
    assert!(
        load_preferences(&path).zone_picked,
        "the pick persists its marker"
    );

    // The offer is not even made against a picked zone.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        None,
        "a picked zone suppresses the offer entirely"
    );

    // And an acceptance against one refuses typed, the picked zone
    // standing on the snapshot and on disk.
    lands(
        &app,
        SettingsIntent::AcceptDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    let snapshot = app.settings();
    assert_eq!(
        snapshot.error,
        Some(SettingsError::ZonePicked),
        "the refusal is typed on the snapshot"
    );
    assert_eq!(snapshot.settings.zone, "Europe/Amsterdam");
    assert_eq!(load_preferences(&path).zone, "Europe/Amsterdam");
}

#[tokio::test]
async fn accepting_the_offer_writes_through_the_zone_picker_s_path() {
    let dir = scratch("accept-offer");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    lands(
        &app,
        SettingsIntent::AcceptDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;

    let snapshot = app.settings();
    assert_eq!(snapshot.settings.zone, "Europe/Berlin");
    assert_eq!(snapshot.error, None, "the acceptance landed");
    assert_eq!(
        snapshot.device_zone_offer, None,
        "the answered offer clears"
    );
    assert_eq!(load_preferences(&path).zone, "Europe/Berlin");

    // The acceptance is not a pick: the marker never moves here, so a
    // later device-zone change may still offer.
    assert!(
        !load_preferences(&path).zone_picked,
        "an accepted offer never marks a pick"
    );
}

#[tokio::test]
async fn an_accept_refuses_an_unresolvable_zone_and_the_zone_stands() {
    let dir = scratch("accept-refusal");
    let path = dir.join("preferences.toml");
    let app = app(Some(path.clone()));

    lands(
        &app,
        SettingsIntent::AcceptDeviceZone {
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
}

#[tokio::test]
async fn an_unresolvable_report_never_offers_and_latches_nothing() {
    let dir = scratch("report-refusal");
    let path = dir.join("preferences.toml");
    let app = app(Some(path));

    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Mars/Olympus".to_owned(),
        },
    )
    .await;
    assert_eq!(app.settings().device_zone_offer, None);

    // The refused report latched nothing: the next valid zone offers.
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        Some("Europe/Berlin".to_owned())
    );
}

#[tokio::test]
async fn a_pick_answers_the_standing_offer() {
    let dir = scratch("pick-answers");
    let path = dir.join("preferences.toml");
    let app = app(Some(path));

    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert!(app.settings().device_zone_offer.is_some());

    // The person ignores the prompt and picks their own zone: the pick
    // answers the standing offer, and no offer follows it.
    lands(
        &app,
        SettingsIntent::SetZone {
            zone: "Europe/Amsterdam".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        None,
        "a pick clears the pending offer"
    );
    lands(
        &app,
        SettingsIntent::ReportDeviceZone {
            zone: "Europe/Berlin".to_owned(),
        },
    )
    .await;
    assert_eq!(
        app.settings().device_zone_offer,
        None,
        "the picked zone suppresses every later offer"
    );
}
