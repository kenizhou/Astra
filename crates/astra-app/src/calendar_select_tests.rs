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

#[tokio::test]
async fn an_appended_calendar_leg_syncs_through_the_second_provider() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let calendar = CalendarFake::with_calendar_and_mail().holding(zoned_event(
        "evt-cal",
        "The real standup",
        "2026-10-19T09:00:00",
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
        start: zoned("2026-10-21T14:00:00"),
        end: zoned("2026-10-21T15:00:00"),
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
    assert!(snapshot.occurrences.is_empty());
}
