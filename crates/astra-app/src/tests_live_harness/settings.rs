// SPDX-License-Identifier: GPL-3.0-only
//! The gated settings proofs against the live harness: the dav editor
//! end to end over the REAL seam (the mail-only start, a SetDav whose
//! re-dial runs through the real `connect_dav_providers`, both legs
//! installed, both surfaces filled, the masked echo back), the failure
//! contract live (an unreachable origin, the old legs standing, the
//! stored section unchanged), the clear (both legs torn down, the store
//! domains forgotten), and the zone consumption live-lite (the now
//! line republished under the new id). The shared fixtures, including
//! the in-suite host account store and its mirroring note, live in
//! `settings_proof`; the seeding discipline is the calendar and people
//! proofs' (raw helpers under unique run tokens, withdrawn at the end,
//! every assertion through the product's own snapshots).

use astra_viewmodel::{AccountKind, AccountLegs};

use crate::protocol::{DavSectionInput, SettingsIntent};
use crate::{CalendarIntent, Intent, PeopleIntent};

use super::calendar_proof::{dav, event_ics, range, rows_of, run_token};
use super::people_proof::{card_href, card_vcf, rows_carrying, seed_card, withdraw_card};
use super::settings_proof::{row_of, save_both_legs, settings_suite};
use super::{harness, jmap_base};

/// The dav editor end to end over the REAL seam: an app whose account
/// started mail-only (no dav section anywhere, the row's honest empty
/// echo), a SetDav naming both origins of the harness's shared listener
/// with the credentials, and the re-dial installs BOTH legs: the
/// appended calendar provider fills the calendar surface with the event
/// the run seeded first, the installed contacts leg fills the people
/// surface with the card the run seeded, and the settings row carries
/// the masked section back with `dav_password_set` and both legs flags.
#[tokio::test]
async fn the_dav_editor_installs_both_legs_end_to_end() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let title = format!("Harness settings plain {token}");
    let event_uid = format!("settings-{token}@astra.local");
    assert_eq!(
        dav(
            "PUT",
            &user,
            &event_uid,
            &password,
            &event_ics(
                &event_uid,
                &title,
                "DTSTART:20261104T100000Z",
                "DTEND:20261104T110000Z",
                &[],
            ),
        ),
        201,
        "the seeding PUT stores the event"
    );
    let card_name = format!("Seta Cardinal {token}");
    let card_uid = format!("settings-card-{token}@astra.local");
    assert_eq!(
        seed_card(
            &user,
            &card_uid,
            &password,
            &card_vcf(
                &card_uid,
                Some(&card_name),
                &[&format!("EMAIL:seta-{token}@example.test")],
            ),
        ),
        201,
        "the seeding PUT stores the card"
    );

    let (app, account, _store) = settings_suite(&host, &port, &user, &password).await;

    // The mail-only start: no dav echo, no legs, the honest row.
    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
    let settings = app.settings();
    let row = row_of(&settings, &account);
    assert!(
        row.dav.is_none(),
        "a mail-only account carries no dav section"
    );
    assert_eq!(
        row.legs,
        AccountLegs {
            calendar: false,
            contacts: false
        },
        "neither leg stands before the save"
    );
    assert_eq!(row.kind, AccountKind::ImapSmtp);
    assert_eq!(row.address, user, "the row names the account's address");

    // The save: a section naming both origins, the login, and the
    // password. The store dials it for real before it persists.
    let base = jmap_base().expect("the gated run names the harness origin");
    save_both_legs(&app, &account, &base, &user, &password).await;

    // The calendar filled: the seeded event renders through the
    // product's own snapshot after one refresh.
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        rows_of(&app.calendar(), &title).len(),
        1,
        "the appended calendar provider filled the calendar"
    );

    // The contacts filled: the seeded card renders, and the installed
    // leg advertises the writable book.
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let people = app.people();
    assert_eq!(
        rows_carrying(&people, &token).len(),
        1,
        "the installed contacts leg filled the people view"
    );
    assert!(
        people
            .create_targets
            .iter()
            .any(|target| target.account == account),
        "the installed contacts leg advertises the account's writable book"
    );

    // The row: the masked echo with the password flag and both legs
    // flags, no sentence standing.
    let settings = app.settings();
    let row = row_of(&settings, &account);
    let echo = row.dav.as_ref().expect("the row echoes the stored section");
    assert_eq!(echo.caldav_url.as_deref(), Some(base.as_str()));
    assert_eq!(echo.carddav_url.as_deref(), Some(base.as_str()));
    assert_eq!(echo.username, user);
    assert!(
        echo.dav_password_set,
        "the echo states a stored password without carrying it"
    );
    assert_eq!(
        row.legs,
        AccountLegs {
            calendar: true,
            contacts: true
        },
        "both legs stand after the save"
    );
    assert!(
        row.dav_error.is_none(),
        "the landed save leaves no sentence"
    );
    assert!(app.settings().error.is_none());

    // The withdrawal: the run leaves the harness's shape as it found it.
    assert_eq!(
        dav("DELETE", &user, &event_uid, &password, ""),
        204,
        "the seeding cleanup withdraws the event"
    );
    assert_eq!(
        withdraw_card(&user, &card_href(&user, &card_uid), &password),
        204,
        "the seeding cleanup withdraws the card"
    );
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_of(&app.calendar(), &title).is_empty(),
        "the withdrawn event renders nowhere after one refresh"
    );
    assert!(
        rows_carrying(&app.people(), &token).is_empty(),
        "the withdrawn card renders nowhere after one refresh"
    );
}

/// The failure contract live: a SetDav whose both origins name a port
/// nothing serves. The dial fails, the OLD legs keep standing (the
/// calendar still renders the seeded event through the provider the
/// good save installed), the row carries `dav_error`, and the stored
/// section is unchanged: a later good save in the editor's blank-keeps
/// shape (no password stated) dials on the credential that survived the
/// failed write and lands cleanly.
#[tokio::test]
async fn a_failed_re_dial_keeps_the_standing_legs_and_the_stored_section() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let title = format!("Harness settings failed {token}");
    let event_uid = format!("settings-failed-{token}@astra.local");
    assert_eq!(
        dav(
            "PUT",
            &user,
            &event_uid,
            &password,
            &event_ics(
                &event_uid,
                &title,
                "DTSTART:20261104T120000Z",
                "DTEND:20261104T130000Z",
                &[],
            ),
        ),
        201,
        "the seeding PUT stores the event"
    );

    let (app, account, _store) = settings_suite(&host, &port, &user, &password).await;
    let base = jmap_base().expect("the gated run names the harness origin");
    save_both_legs(&app, &account, &base, &user, &password).await;
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        rows_of(&app.calendar(), &title).len(),
        1,
        "the good save installed the calendar leg"
    );

    // The failed save: both origins on a port nothing serves. Loopback
    // refuses the dial at once.
    let dead = "http://127.0.0.1:9";
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: account.clone(),
        section: Some(DavSectionInput {
            caldav_url: Some(dead.to_owned()),
            carddav_url: Some(dead.to_owned()),
            username: Some(user.clone()),
            password: None,
        }),
    }))
    .await
    .unwrap();

    let settings = app.settings();
    let row = row_of(&settings, &account);
    assert!(
        row.dav_error.is_some(),
        "the failed dial lands on the account's row"
    );
    let echo = row.dav.as_ref().expect("the standing section still echoes");
    assert_eq!(
        echo.caldav_url.as_deref(),
        Some(base.as_str()),
        "the stored section kept the origin that stood"
    );

    // The old legs keep standing: the calendar still renders the seeded
    // event, and the row's legs flags stay true.
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        rows_of(&app.calendar(), &title).len(),
        1,
        "the standing calendar leg still renders the event"
    );
    assert_eq!(
        row.legs,
        AccountLegs {
            calendar: true,
            contacts: true
        }
    );

    // The stored section is unchanged: a subsequent good save stating no
    // password (blank keeps the stored credential) lands cleanly.
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: account.clone(),
        section: Some(DavSectionInput {
            caldav_url: Some(base.clone()),
            carddav_url: Some(base),
            username: None,
            password: None,
        }),
    }))
    .await
    .unwrap();
    let settings = app.settings();
    let row = row_of(&settings, &account);
    assert!(row.dav_error.is_none(), "the good save clears the sentence");
    assert_eq!(
        row.legs,
        AccountLegs {
            calendar: true,
            contacts: true
        },
        "both legs stand after the clean re-save"
    );

    // The withdrawal.
    assert_eq!(
        dav("DELETE", &user, &event_uid, &password, ""),
        204,
        "the seeding cleanup withdraws the event"
    );
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_of(&app.calendar(), &title).is_empty(),
        "the withdrawn event renders nowhere after one refresh"
    );
}

/// The clear: SetDav None tears both legs down. The calendar renders
/// empty for the account (its calendars and occurrences forgotten with
/// the leg), the contacts leg answers nothing (no card renders, no
/// writable book is advertised), and the stored section drops: the row
/// carries no echo and no legs flags.
#[tokio::test]
async fn clearing_the_section_tears_both_legs_down() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let title = format!("Harness settings clear {token}");
    let event_uid = format!("settings-clear-{token}@astra.local");
    assert_eq!(
        dav(
            "PUT",
            &user,
            &event_uid,
            &password,
            &event_ics(
                &event_uid,
                &title,
                "DTSTART:20261104T140000Z",
                "DTEND:20261104T150000Z",
                &[],
            ),
        ),
        201,
        "the seeding PUT stores the event"
    );
    let card_name = format!("Cleo Cleared {token}");
    let card_uid = format!("settings-clear-card-{token}@astra.local");
    assert_eq!(
        seed_card(
            &user,
            &card_uid,
            &password,
            &card_vcf(
                &card_uid,
                Some(&card_name),
                &[&format!("EMAIL:cleo-{token}@example.test")],
            ),
        ),
        201,
        "the seeding PUT stores the card"
    );

    let (app, account, _store) = settings_suite(&host, &port, &user, &password).await;
    let base = jmap_base().expect("the gated run names the harness origin");
    save_both_legs(&app, &account, &base, &user, &password).await;
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(rows_of(&app.calendar(), &title).len(), 1);
    assert_eq!(rows_carrying(&app.people(), &token).len(), 1);

    // The clear.
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: account.clone(),
        section: None,
    }))
    .await
    .unwrap();
    let settings = app.settings();
    let row = row_of(&settings, &account);
    assert!(row.dav.is_none(), "the stored section dropped");
    assert_eq!(
        row.legs,
        AccountLegs {
            calendar: false,
            contacts: false
        },
        "the clear tears both legs down"
    );
    assert!(row.dav_error.is_none(), "a landed clear leaves no sentence");

    // The calendar renders empty for the account: no calendar row, no
    // occurrence, after a refresh that has no calendar leg to sync.
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let calendar = app.calendar();
    assert!(
        !calendar
            .calendars
            .iter()
            .any(|calendar| calendar.account == account),
        "the account lists no calendar after the clear"
    );
    assert!(
        rows_of(&calendar, &title).is_empty(),
        "the cleared account's event renders nowhere"
    );

    // The contacts leg answers nothing.
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let people = app.people();
    assert!(
        rows_carrying(&people, &token).is_empty(),
        "the cleared account's card renders nowhere"
    );
    assert!(
        !people
            .create_targets
            .iter()
            .any(|target| target.account == account),
        "no writable book is advertised after the clear"
    );

    // The withdrawal.
    assert_eq!(
        dav("DELETE", &user, &event_uid, &password, ""),
        204,
        "the seeding cleanup withdraws the event"
    );
    assert_eq!(
        withdraw_card(&user, &card_href(&user, &card_uid), &password),
        204,
        "the seeding cleanup withdraws the card"
    );
}

/// The zone consumption live-lite: a SetZone to Europe/Amsterdam
/// republishes the calendar's now line with the new zone id, the one
/// consumption every floating and all-day instant resolves through. No
/// event maths: the id on the snapshot is the whole proof.
#[tokio::test]
async fn a_zone_change_republishes_the_now_line_s_zone() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, _account, _store) = settings_suite(&host, &port, &user, &password).await;
    // A window first, so the calendar has published a snapshot at all.
    app.dispatch(range()).await.unwrap();
    assert_eq!(
        app.calendar()
            .now_data
            .as_ref()
            .expect("the rebuild published the now line")
            .zone,
        "Etc/UTC",
        "the shipped default zone lays the diary out"
    );
    app.dispatch(Intent::Settings(SettingsIntent::SetZone {
        zone: "Europe/Amsterdam".to_owned(),
    }))
    .await
    .unwrap();
    assert_eq!(
        app.calendar()
            .now_data
            .as_ref()
            .expect("the zone change republished the calendar")
            .zone,
        "Europe/Amsterdam",
        "the now line carries the new zone id"
    );
    assert_eq!(app.settings().settings.zone, "Europe/Amsterdam");
    assert!(app.settings().error.is_none());
}
