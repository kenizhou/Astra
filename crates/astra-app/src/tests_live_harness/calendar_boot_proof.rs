// SPDX-License-Identifier: GPL-3.0-only
//! The gated boot-fetch proof against the live harness: the diary fills
//! without the view ever opening, over the real CalDAV adapter. One event is
//! seeded onto astra1's calendar through the raw CalDAV helper (an event two
//! days out, so it lands inside the cold window whatever day the run runs),
//! the account is dialed in the boot's own shape (the mail providers from the
//! product's certificate walk, the CalDAV leg appended after, the order the
//! host's dial folds them in), and the boot's join site runs with no calendar
//! refresh dispatched before it: the fetch half is the engine's store holding
//! the event afterwards, and the paint half is one plain refresh showing it in
//! the snapshot. The fetch-without-publish shape is pinned offline
//! (calendar_boot_fetch_tests); this proof pins the real-server fetch. The
//! cleanup withdraws the seed, so the calendar keeps the shape the diary
//! suites pin.

use std::sync::Arc;

use engine_api::{EmailAddress, Engine, Provider};
use provider_caldav::{CalDavConfig, CalDavProvider, Credentials};

use crate::{Account, App, AppObserver, CalendarIntent, Intent};

use super::calendar_proof::{dav, event_ics, rows_of, run_token, utc_in_days};
use super::{dial_with_accepted_certificate, harness, CountingObserver};

/// The boot's join fills the diary over the real server: an event seeded onto
/// the account's calendar lands in the engine's store through the boot's own
/// join with no refresh dispatched, and one plain refresh paints it.
#[tokio::test]
async fn the_boot_join_fetches_the_calendar_without_the_view_open() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let summary = format!("Harness boot fetch {token}");
    let uid = format!("boot-{token}@astra.local");
    let start = utc_in_days(2, 10, 0);
    let end = utc_in_days(2, 11, 0);
    assert_eq!(
        dav(
            "PUT",
            &user,
            &uid,
            &password,
            &event_ics(
                &uid,
                &summary,
                &format!("DTSTART:{start}"),
                &format!("DTEND:{end}"),
                &[]
            ),
        ),
        201,
        "the seeding PUT stores the event"
    );

    // The boot's own account shape: the CalDAV leg dialed beside the mail
    // providers and appended after them, one engine store under both.
    let caldav = CalDavProvider::connect(CalDavConfig::new(
        super::jmap_base().expect("the gated run names the harness origin"),
        Credentials::Basic {
            username: user.clone(),
            password: password.clone(),
        },
    ))
    .await
    .expect("the CalDAV dial and discovery");
    let (first, mut providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    providers.push(Box::new(caldav) as Box<dyn Provider>);
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: first.clone(),
            providers,
            identity: EmailAddress::new(&user),
        }],
        None,
        observer,
    );

    // The boot's join site itself: mail and calendar side by side, with no
    // calendar refresh ever dispatched before it.
    app.sync_added_account(&first).await;

    // The fetch half: the store holds the seeded event, read through the
    // engine's own store, before any refresh has run.
    let stored = app.engine.events(&first).await.expect("the store reads");
    let seeded = stored
        .iter()
        .find(|event| event.uid.as_str() == uid)
        .unwrap_or_else(|| panic!("the boot join fetched the seeded event into the store"));
    assert_eq!(
        seeded.title, summary,
        "the fetched event is the event the seed stored"
    );

    // The paint half: one plain refresh and the snapshot shows it.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.calendar();
    let rows = rows_of(&snapshot, &summary);
    assert_eq!(
        rows.len(),
        1,
        "the fetched event renders in the snapshot after one plain refresh"
    );

    // The cleanup: the seed withdrawn, the diary back to the shape the suites
    // pin.
    assert_eq!(
        dav("DELETE", &user, &uid, &password, ""),
        204,
        "the seeding cleanup withdraws the event"
    );
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.calendar();
    assert!(
        rows_of(&snapshot, &summary).is_empty(),
        "the withdrawn event renders nowhere after one refresh"
    );
}
