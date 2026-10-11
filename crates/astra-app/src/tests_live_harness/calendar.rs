// SPDX-License-Identifier: GPL-3.0-only
//! The gated calendar diary proofs against the live harness: the sync
//! read (the seeded calendars and occurrences materialise with
//! occurrence-accurate times) and the write round trip (create, title
//! edit, drag move, the content-only occurrence edit that fills both
//! clocks unchanged, and deletes, every write state settling by the time
//! its dispatch returns). The shared fixtures live in `calendar_proof`;
//! the invitation machine's proof and the relayed meeting brief sit in
//! their own files beside this one. The composition note (two apps over
//! one engine store, the mail leg on the IMAP dial and the calendar leg
//! on the real CalDAV adapter) and the seeding discipline (self-seeded,
//! self-cleaned, unique run tokens) are recorded there.

use engine_api::Frequency;

use crate::protocol::{RepeatEnd, RepeatRule};
use crate::reference::EventRef;
use crate::{CalendarIntent, EventDrag, EventEdge, EventEdit, Intent};

use super::calendar_proof::{
    dav, default_calendar, event_ics, event_of_account, range, rows_of, run_token, suite_apps,
    zoned,
};
use super::harness;

/// A calendar sync over the running server materialises the seeded
/// calendars and occurrences with occurrence-accurate times: a plain
/// event, a weekly recurring one, and an all-day one, each read back
/// through the product's own snapshot after one refresh.
#[tokio::test]
async fn the_calendar_sync_materialises_the_seeded_events() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let plain = format!("Harness plain {token}");
    let weekly = format!("Harness weekly {token}");
    let allday = format!("Harness allday {token}");
    let uids: Vec<String> = ["plain", "weekly", "allday"]
        .iter()
        .map(|kind| format!("{kind}-{token}@astra.local"))
        .collect();
    let seeds = [
        event_ics(
            &uids[0],
            &plain,
            "DTSTART:20261103T100000Z",
            "DTEND:20261103T110000Z",
            &[],
        ),
        event_ics(
            &uids[1],
            &weekly,
            "DTSTART:20261102T090000Z",
            "DTEND:20261102T093000Z",
            &["RRULE:FREQ=WEEKLY;COUNT=3"],
        ),
        event_ics(
            &uids[2],
            &allday,
            "DTSTART;VALUE=DATE:20261105",
            "DTEND;VALUE=DATE:20261106",
            &[],
        ),
    ];
    for (uid, bytes) in uids.iter().zip(&seeds) {
        assert_eq!(
            dav("PUT", &user, uid, &password, bytes),
            201,
            "the seeding PUT stores the event"
        );
    }

    let (_mail, calendar, _first, _second) = suite_apps(&host, &port, &user, &password).await;
    calendar.dispatch(range()).await.unwrap();
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &plain);
    assert_eq!(rows.len(), 1, "the plain event renders once");
    assert_eq!(
        (rows[0].start.to_string(), rows[0].end.to_string()),
        (
            "2026-11-03T10:00:00Z".to_owned(),
            "2026-11-03T11:00:00Z".to_owned()
        ),
        "the plain occurrence carries its own instants"
    );
    assert!(!rows[0].recurring && !rows[0].all_day);

    let rows = rows_of(&snapshot, &weekly);
    assert_eq!(rows.len(), 3, "the weekly series renders three times");
    assert_eq!(
        rows.iter()
            .map(|row| row.start.to_string())
            .collect::<Vec<_>>(),
        [
            "2026-11-02T09:00:00Z".to_owned(),
            "2026-11-09T09:00:00Z".to_owned(),
            "2026-11-16T09:00:00Z".to_owned()
        ],
        "each occurrence carries its own start"
    );
    assert!(rows
        .iter()
        .all(|row| row.recurring && row.occurrence_start.is_some()));
    assert_eq!(
        rows[1].occurrence_start.map(|clock| clock.to_string()),
        Some("2026-11-09T09:00:00".to_owned()),
        "the second occurrence's token names its own original start"
    );

    let rows = rows_of(&snapshot, &allday);
    assert_eq!(rows.len(), 1, "the all-day event renders once");
    assert!(rows[0].all_day, "the all-day marker rides the row");
    assert_eq!(
        rows[0].start.to_string(),
        "2026-11-05T00:00:00Z",
        "an all-day occurrence resolves through the display zone"
    );

    for uid in &uids {
        assert_eq!(
            dav("DELETE", &user, uid, &password, ""),
            204,
            "the seeding cleanup withdraws the event"
        );
    }
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    assert!(
        [&plain, &weekly, &allday]
            .iter()
            .all(|title| rows_of(&snapshot, title).is_empty()),
        "the withdrawn events render nowhere after one refresh"
    );
}

/// The write family round-tripped live: a create lands in the snapshot, a
/// title edit and a drag move land, a content-only occurrence edit fills
/// both clocks unchanged on the recurring series (the occurrence-clocks
/// contract), deletes withdraw, and every write state settles by the time
/// its dispatch returns.
#[tokio::test]
async fn the_calendar_round_trip_lands_every_write() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let plain = format!("Harness trip {token}");
    let series = format!("Harness standup {token}");
    let renamed = format!("Harness standup moved {token}");
    let (_mail, calendar, first, _second) = suite_apps(&host, &port, &user, &password).await;
    calendar.dispatch(range()).await.unwrap();
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    // The create, through the intent family, under the core-minted UID.
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::CreateEvent {
            calendar: default_calendar(&calendar, &first),
            summary: plain.clone(),
            start: zoned("2026-11-05T12:00:00"),
            end: zoned("2026-11-05T13:00:00"),
            description: None,
            location: None,
            recurrence: None,
        }))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &plain);
    assert_eq!(rows.len(), 1, "the created event renders at once");
    assert_eq!(
        rows[0].start.to_string(),
        "2026-11-05T12:00:00Z",
        "the created occurrence starts where the create said"
    );

    // The title edit, on the series scope a one-off event is.
    let edited = format!("{plain} edited");
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
            event: event_of_account(&calendar, &first, &plain),
            edit: EventEdit {
                title: Some(edited.clone()),
                ..EventEdit::default()
            },
        }))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &edited);
    assert_eq!(rows.len(), 1, "the edit landed");
    assert_eq!(
        rows[0].start.to_string(),
        "2026-11-05T12:00:00Z",
        "a title edit moves nothing"
    );

    // The move: a whole-edge drag of one day and thirty minutes, on the
    // event's own wall clock.
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::MoveEvent {
            event: event_of_account(&calendar, &first, &edited),
            drag: EventDrag {
                edge: EventEdge::Whole,
                days: 1,
                minutes: 30,
                occurrence: None,
            },
        }))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &edited);
    assert_eq!(rows.len(), 1, "the moved event renders once");
    assert_eq!(
        (rows[0].start.to_string(), rows[0].end.to_string()),
        (
            "2026-11-06T12:30:00Z".to_owned(),
            "2026-11-06T13:30:00Z".to_owned()
        ),
        "the drag moved both edges, duration intact"
    );

    // A recurring series through the same create path.
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::CreateEvent {
            calendar: default_calendar(&calendar, &first),
            summary: series.clone(),
            start: zoned("2026-11-03T09:00:00"),
            end: zoned("2026-11-03T09:30:00"),
            description: None,
            location: None,
            recurrence: Some(RepeatRule {
                frequency: Frequency::Weekly,
                interval: 1,
                weekdays: Vec::new(),
                end: RepeatEnd::Never,
            }),
        }))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &series);
    assert_eq!(rows.len(), 3, "the created series renders three times");
    let second = rows[1];
    assert!(!second.overridden, "no occurrence is overridden yet");
    let second_clock = second.occurrence_start.expect("the occurrence token");

    // The content-only occurrence edit: a title for the second occurrence,
    // no clocks stated, so the app must fill both unchanged from the
    // store. The live regression pin for the occurrence-clocks fill: a
    // first-touch override without both clocks is refused by the engine's
    // CalDAV path, so the override landing proves the fill ran. The
    // occurrence's own title lives in the override the engine holds (read
    // back through the engine's targeted master read, the same read the
    // brief drive rides); the snapshot's row still draws the master's
    // title, the recorded gap the docs carry.
    let series_key = event_of_account(&calendar, &first, &series).key;
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::UpdateEvent {
            event: EventRef::from_parts(first.as_str(), series_key.as_str().to_owned())
                .expect("the series reference"),
            edit: EventEdit {
                title: Some(renamed.clone()),
                occurrence: Some(second_clock),
                ..EventEdit::default()
            },
        }))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &series);
    assert_eq!(rows.len(), 3, "the series still renders three times");
    assert!(
        rows[1].overridden,
        "the edited occurrence now renders as the override it became"
    );
    assert_eq!(
        (rows[1].start.to_string(), rows[1].end.to_string()),
        (
            "2026-11-10T09:00:00Z".to_owned(),
            "2026-11-10T09:30:00Z".to_owned()
        ),
        "the content-only edit moved neither clock"
    );
    assert!(
        !rows[0].overridden && !rows[2].overridden,
        "the unedited occurrences stay plain instances"
    );
    let stored = calendar
        .engine
        .events_by_keys(&first, std::slice::from_ref(&series_key))
        .await
        .expect("the master read")
        .into_iter()
        .next()
        .expect("the master is stored");
    assert!(
        stored
            .recurrence
            .as_ref()
            .is_some_and(|recurrence| recurrence.overrides.contains_key(&second_clock)),
        "the override the edit wrote is stored under the occurrence token"
    );

    // Every write settled: no write row stands for the account.
    assert!(
        calendar.calendar().write_states.is_empty(),
        "every write state settled by the time its dispatch returned"
    );

    // The deletes: the series whole, the one-off whole.
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
            event: EventRef::from_parts(first.as_str(), series_key.as_str().to_owned())
                .expect("the series reference"),
            occurrence: None,
        }))
        .await
        .unwrap();
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
            event: event_of_account(&calendar, &first, &edited),
            occurrence: None,
        }))
        .await
        .unwrap();
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    assert!(
        [&edited, &series]
            .iter()
            .all(|title| rows_of(&snapshot, title).is_empty()),
        "the deleted events render nowhere after one refresh"
    );
}
