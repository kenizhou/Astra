// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar provider selection: the capability-scoped pick the
//! calendar paths make among an account's providers. An account whose
//! calendar leg was appended after its mail providers must sync and write
//! through the second provider, and an account with no calendar provider at
//! all must behave exactly as it did when the first provider was the only
//! pick there was.

use std::sync::{Arc, Mutex};

use engine_api::{CalendarDateTime, EmailAddress, TimeZoneId};

use crate::calendar_fakes::{calendar_app, local, zoned_event, CalendarFake};
use crate::{reference::CalendarRef, Account, CalendarIntent, Intent};

/// The mail half of a split account: a provider with no calendar leg, the
/// shape every mail provider has. Its calendar passes refuse the way the
/// provider trait's own defaults do for a domain an adapter does not speak.
fn mail_half() -> CalendarFake {
    CalendarFake::with_calendar_and_mail().mail_only()
}

/// The account shape a dav dial produces: the mail providers first, the
/// calendar provider appended after.
fn split_account(id: &str, mail: CalendarFake, calendar: CalendarFake) -> Account<CalendarFake> {
    Account {
        id: engine_api::AccountId::try_from(id).unwrap(),
        providers: vec![mail, calendar],
        identity: EmailAddress::new("info@test.local"),
    }
}

/// A wall clock in Amsterdam, the way the write literal states one.
fn zoned(value: &str) -> CalendarDateTime {
    CalendarDateTime::Zoned {
        local: local(value),
        zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
    }
}

/// A wall-clock literal `days` from today at `hour`, so the fixtures sit
/// inside the default window whatever day the suite runs: a fixed past
/// date ages out of the now-relative horizon and the occurrence silently
/// stops rendering.
fn in_days(days: i64, hour: u8) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock reads after the epoch")
        .as_secs()
        .cast_signed();
    let day = secs.div_euclid(86_400) + days;
    // Howard Hinnant's civil_from_days: the day number to (y, m, d).
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:00:00")
}

#[tokio::test]
async fn an_appended_calendar_leg_syncs_through_the_second_provider() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let calendar = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-cal",
        "The real standup",
        &in_days(7, 9),
    ));
    let app = calendar_app(
        vec![split_account("work", mail_half(), calendar)],
        &surfaces,
    );

    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.calendar();
    assert!(
        snapshot.calendars.iter().any(|row| row.name == "Work"),
        "the calendar the appended provider serves is the one that landed: {:?}",
        snapshot.calendars
    );
    assert!(
        snapshot
            .occurrences
            .iter()
            .any(|row| row.title == "The real standup"),
        "the appended provider's events landed through it"
    );
}

#[tokio::test]
async fn a_write_rides_the_appended_calendar_leg_too() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let calendar = CalendarFake::with_calendar_and_mail();
    let creates = Arc::clone(&calendar.creates);
    let app = calendar_app(
        vec![split_account("work", mail_half(), calendar)],
        &surfaces,
    );
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::CreateEvent {
        calendar: CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap(),
        summary: "Review".to_owned(),
        start: zoned(&in_days(9, 14)),
        end: zoned(&in_days(9, 15)),
        description: None,
        location: None,
        recurrence: None,
    }))
    .await
    .unwrap();

    assert_eq!(
        creates.lock().unwrap().len(),
        1,
        "the create went through the provider that can take it"
    );
    assert!(
        app.calendar()
            .occurrences
            .iter()
            .any(|row| row.title == "Review"),
        "the written event joined the snapshot the appended provider serves"
    );
}

#[tokio::test]
async fn an_account_with_no_calendar_provider_stays_exactly_as_it_was() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = calendar_app(
        vec![split_account(
            "work",
            mail_half(),
            CalendarFake::with_calendar_and_mail().mail_only(),
        )],
        &surfaces,
    );

    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.calendar();
    assert!(
        snapshot.calendars.is_empty(),
        "no provider of the account serves calendars, so none landed"
    );
    assert_eq!(
        snapshot.occurrences,
        [] as [astra_viewmodel::OccurrenceRow; 0]
    );
}
