// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the dav editor: a good section re-dials through the seam and
//! installs both legs, a failed re-dial keeps the old legs standing and
//! answers on the row, a carddav-only section stands, and clearing the
//! section tears the legs down with their store domains.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{AccountLegs, DavSectionRow};

use crate::calendar_fakes::CalendarFake;
use crate::protocol::{CalendarIntent, DavSectionInput, SettingsIntent};
use crate::settings_fakes::{settings_app, FakeAccountOps};
use crate::{App, Intent, Surface};
use engine_api::AccountId;

/// A fresh section naming the CalDAV origin, the editor's first save.
fn both_origins() -> DavSectionInput {
    DavSectionInput {
        caldav_url: Some("https://dav.example.test".to_owned()),
        carddav_url: None,
        username: Some("ada".to_owned()),
        password: Some("app-password".to_owned()),
    }
}

/// The `work` account's row in the settings snapshot.
fn row(app: &App<CalendarFake>) -> astra_viewmodel::AccountSettingsRow {
    app.settings()
        .accounts
        .into_iter()
        .find(|row| row.account.as_str() == "work")
        .expect("the work account has a row")
}

/// Dispatches the section save and asserts it landed.
async fn save(app: &App<CalendarFake>, section: Option<DavSectionInput>) {
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: AccountId::try_from("work").unwrap(),
        section,
    }))
    .await
    .unwrap();
}

/// One mail-half account named `work`, the shape the editor starts from,
/// with the scripted store shared back to the test.
fn app() -> (
    App<CalendarFake>,
    Arc<Mutex<Vec<Surface>>>,
    Arc<FakeAccountOps>,
) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let ops = Arc::new(FakeAccountOps::new().describing("work"));
    let app = settings_app(
        vec![crate::settings_fakes::account(
            "work",
            vec![crate::settings_fakes::mail_half()],
        )],
        Arc::clone(&ops),
        None,
        &surfaces,
    );
    (app, surfaces, ops)
}

#[tokio::test]
async fn a_good_section_re_dials_and_installs_both_legs() {
    let (app, surfaces, ops) = app();
    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        row(&app).legs,
        AccountLegs {
            calendar: false,
            contacts: false
        },
        "the mail half starts with no dav legs"
    );

    save(&app, Some(both_origins())).await;

    // The stored config holds the merged section and the row echoes it
    // masked: the username and the origins, and only the fact of a password.
    assert_eq!(ops.stored_section("work"), Some("ada".to_owned()));
    assert_eq!(
        row(&app).dav,
        Some(DavSectionRow {
            caldav_url: Some("https://dav.example.test".to_owned()),
            carddav_url: None,
            username: "ada".to_owned(),
            dav_password_set: true,
        }),
        "the echo never carries the credential"
    );
    assert_eq!(row(&app).dav_error, None);
    assert_eq!(
        row(&app).legs,
        AccountLegs {
            calendar: true,
            contacts: true
        },
        "the re-dial appended the calendar provider and installed the contacts leg"
    );

    // The standing calendar leg serves a sync, so the appended provider is
    // live: the account's calendars join the snapshot.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(
        app.calendar()
            .calendars
            .iter()
            .any(|calendar| calendar.account.as_str() == "work"),
        "the dialled calendar leg serves the calendar surface"
    );
    // The installed contacts leg advertises its book to the people surface,
    // once a people refresh syncs it into the store.
    app.dispatch(Intent::People(crate::protocol::PeopleIntent::Refresh))
        .await
        .unwrap();
    assert!(
        app.people()
            .create_targets
            .iter()
            .any(|target| target.account.as_str() == "work"),
        "the installed contacts leg serves the people surface"
    );
    let signalled = surfaces.lock().unwrap().clone();
    for surface in [Surface::Settings, Surface::Calendar, Surface::People] {
        assert!(signalled.contains(&surface), "{surface:?} was published");
    }
}

#[tokio::test]
async fn a_failed_re_dial_keeps_the_old_legs_and_answers_on_the_row() {
    let (app, _surfaces, ops) = app();
    save(&app, Some(both_origins())).await;
    let standing = row(&app).dav.clone().expect("the first save stood");

    ops.failing_dial("work", "the origin could not be reached");
    save(
        &app,
        Some(DavSectionInput {
            username: Some("ada2".to_owned()),
            ..both_origins()
        }),
    )
    .await;

    assert_eq!(
        ops.stored_section("work"),
        Some("ada".to_owned()),
        "the failed dial stored nothing"
    );
    assert_eq!(row(&app).dav, Some(standing), "the standing section stands");
    assert_eq!(
        row(&app).dav_error,
        Some("the origin could not be reached".to_owned()),
        "the row says why the save did not land"
    );
    assert_eq!(
        row(&app).legs,
        AccountLegs {
            calendar: true,
            contacts: true
        },
        "the old legs keep standing"
    );
}

#[tokio::test]
async fn a_section_naming_no_origin_is_refused() {
    let (app, _surfaces, _ops) = app();
    save(
        &app,
        Some(DavSectionInput {
            caldav_url: Some("   ".to_owned()),
            carddav_url: None,
            username: Some("ada".to_owned()),
            password: Some("app-password".to_owned()),
        }),
    )
    .await;
    assert_eq!(
        row(&app).dav_error,
        Some("the section names no origin to dial".to_owned())
    );
    assert!(row(&app).dav.is_none(), "nothing was stored");
    assert_eq!(
        row(&app).legs,
        AccountLegs {
            calendar: false,
            contacts: false
        }
    );
}

#[tokio::test]
async fn a_carddav_only_section_stands_on_the_contacts_leg_alone() {
    let (app, _surfaces, _ops) = app();
    save(
        &app,
        Some(DavSectionInput {
            caldav_url: None,
            carddav_url: Some("https://cards.example.test".to_owned()),
            username: Some("ada".to_owned()),
            password: Some("app-password".to_owned()),
        }),
    )
    .await;
    assert_eq!(
        row(&app).legs,
        AccountLegs {
            calendar: false,
            contacts: true
        },
        "a carddav-only section dials no calendar leg"
    );
    assert_eq!(
        row(&app)
            .dav
            .as_ref()
            .and_then(|dav| dav.carddav_url.clone()),
        Some("https://cards.example.test".to_owned())
    );
}

#[tokio::test]
async fn clearing_the_section_tears_the_legs_and_their_domains_down() {
    let (app, surfaces, ops) = app();
    save(&app, Some(both_origins())).await;
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(!app.calendar().calendars.is_empty());

    save(&app, None).await;

    assert_eq!(
        ops.stored_section("work"),
        None,
        "the config dropped the section"
    );
    assert!(row(&app).dav.is_none(), "the row echoes no section");
    assert_eq!(row(&app).dav_error, None);
    assert_eq!(
        row(&app).legs,
        AccountLegs {
            calendar: false,
            contacts: false
        },
        "both legs tore down"
    );
    assert!(
        app.calendar()
            .calendars
            .iter()
            .all(|calendar| calendar.account.as_str() != "work"),
        "the calendar domain forgot the leg's rows"
    );
    assert!(
        app.people().create_targets.is_empty(),
        "the contacts domain forgot the leg's rows"
    );
    assert!(surfaces.lock().unwrap().contains(&Surface::Settings));
}
