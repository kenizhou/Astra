// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar half of the inbound protocol: the exact JSON wire shape every
//! [`CalendarIntent`](super::CalendarIntent) crosses the IPC in, that each reads back
//! identical, and the additive `calendar` surface word.

use engine_api::{
    CalendarDate, CalendarDateTime, Frequency, LocalDateTime, TimeZoneId, UtcDateTime, Weekday,
};

use super::{
    CalendarIntent, EventDrag, EventEdge, EventEdit, Intent, RecurrenceChange, RepeatEnd,
    RepeatRule, RsvpAnswer, Surface,
};
use crate::reference::{CalendarRef, EventRef, MessageRef};

/// A well-formed account id, the way every calendar test names one.
fn account(value: &str) -> engine_api::AccountId {
    engine_api::AccountId::try_from(value).unwrap()
}

/// A wall clock, the way the drag and edit literals name one.
fn local(value: &str) -> LocalDateTime {
    value.parse().unwrap()
}

/// A UTC instant, the way the range literal names one.
fn utc(value: &str) -> UtcDateTime {
    value.parse().unwrap()
}

/// A calendar reference bound to the work account.
fn calendar() -> CalendarRef {
    CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap()
}

/// An event reference bound to the work account.
fn event() -> EventRef {
    EventRef::from_parts("work", "/dav/cal/work/default/evt-1.ics".to_owned()).unwrap()
}

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

#[test]
fn refresh_and_set_date_cross_as_bare_words() {
    round_trips(
        &Intent::Calendar(CalendarIntent::Refresh),
        r#"{"calendar":"refresh"}"#,
    );
    round_trips(
        &Intent::Calendar(CalendarIntent::SetDate {
            date: CalendarDate::new(2026, 10, 19).unwrap(),
        }),
        r#"{"calendar":{"set_date":{"date":"2026-10-19"}}}"#,
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<Intent>(r#"{"calendar":"Refresh"}"#).is_err());
}

#[test]
fn set_range_carries_the_window_anchor_and_its_length() {
    round_trips(
        &Intent::Calendar(CalendarIntent::SetRange {
            start: utc("2026-10-18T22:00:00Z"),
            days: 7,
        }),
        r#"{"calendar":{"set_range":{"start":"2026-10-18T22:00:00Z","days":7}}}"#,
    );
}

#[test]
fn set_visible_calendars_names_each_calendar_by_reference() {
    round_trips(
        &Intent::Calendar(CalendarIntent::SetVisibleCalendars {
            ids: vec![
                CalendarRef::from_parts("work", "cal-1".to_owned()).unwrap(),
                CalendarRef::from_parts("home", "personal".to_owned()).unwrap(),
            ],
        }),
        concat!(
            r#"{"calendar":{"set_visible_calendars":{"ids":"#,
            r#"[{"account":"work","key":"cal-1"},{"account":"home","key":"personal"}]}}}"#
        ),
    );
}

#[test]
fn a_create_carries_the_engine_shaped_fields_and_skips_what_is_absent() {
    // The minimal create: a one-off, no notes, no location, no rule. The optional fields
    // cross no key at all, exactly as a folder create omits its absent parent. The start
    // and end are the engine's own `CalendarDateTime`, crossing verbatim in the engine's
    // representation (a tagged form naming its zone), as the composer document crosses in
    // its own block shapes inside the drafts intent.
    round_trips(
        &Intent::Calendar(CalendarIntent::CreateEvent {
            calendar: calendar(),
            summary: "Standup".to_owned(),
            start: CalendarDateTime::Zoned {
                local: local("2026-10-19T09:00:00"),
                zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
            },
            end: CalendarDateTime::Zoned {
                local: local("2026-10-19T09:30:00"),
                zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
            },
            description: None,
            location: None,
            recurrence: None,
        }),
        concat!(
            r#"{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"#,
            r#""summary":"Standup","#,
            r#""start":{"Zoned":{"local":"2026-10-19T09:00:00","#,
            r#""zone":{"Iana":"Europe/Amsterdam"}}},"#,
            r#""end":{"Zoned":{"local":"2026-10-19T09:30:00","#,
            r#""zone":{"Iana":"Europe/Amsterdam"}}}}}}"#
        ),
    );
    // The full create: notes, a location, and a weekly rule ending after five instances.
    round_trips(
        &Intent::Calendar(CalendarIntent::CreateEvent {
            calendar: calendar(),
            summary: "Standup".to_owned(),
            start: CalendarDateTime::Zoned {
                local: local("2026-10-19T09:00:00"),
                zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
            },
            end: CalendarDateTime::Zoned {
                local: local("2026-10-19T09:30:00"),
                zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
            },
            description: Some("Weekly sync".to_owned()),
            location: Some("Room A".to_owned()),
            recurrence: Some(RepeatRule {
                frequency: Frequency::Weekly,
                interval: 1,
                weekdays: vec![Weekday::Mo],
                end: RepeatEnd::AfterCount(5),
            }),
        }),
        concat!(
            r#"{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"#,
            r#""summary":"Standup","#,
            r#""start":{"Zoned":{"local":"2026-10-19T09:00:00","#,
            r#""zone":{"Iana":"Europe/Amsterdam"}}},"#,
            r#""end":{"Zoned":{"local":"2026-10-19T09:30:00","#,
            r#""zone":{"Iana":"Europe/Amsterdam"}}},"#,
            r#""description":"Weekly sync","location":"Room A","#,
            r#""recurrence":{"frequency":"weekly","interval":1,"weekdays":["mo"],"#,
            r#""end":{"after_count":5}}}}}"#
        ),
    );
}

#[test]
fn a_repeat_rule_ends_never_on_a_date_or_after_a_count() {
    let rule = |end| RepeatRule {
        frequency: Frequency::Daily,
        interval: 2,
        weekdays: Vec::new(),
        end,
    };
    let as_recurrence = |rule| {
        Intent::Calendar(CalendarIntent::CreateEvent {
            calendar: calendar(),
            summary: String::new(),
            start: CalendarDateTime::Date(CalendarDate::new(2026, 10, 19).unwrap()),
            end: CalendarDateTime::Date(CalendarDate::new(2026, 10, 20).unwrap()),
            description: None,
            location: None,
            recurrence: Some(rule),
        })
    };
    round_trips(
        &as_recurrence(rule(RepeatEnd::Never)),
        concat!(
            r#"{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"#,
            r#""summary":"","start":{"Date":"2026-10-19"},"end":{"Date":"2026-10-20"},"#,
            r#""recurrence":{"frequency":"daily","interval":2,"weekdays":[],"end":"never"}}}}"#
        ),
    );
    round_trips(
        &as_recurrence(rule(RepeatEnd::OnDate(
            CalendarDate::new(2026, 12, 18).unwrap(),
        ))),
        concat!(
            r#"{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"#,
            r#""summary":"","start":{"Date":"2026-10-19"},"end":{"Date":"2026-10-20"},"#,
            r#""recurrence":{"frequency":"daily","interval":2,"weekdays":[],"#,
            r#""end":{"on_date":"2026-12-18"}}}}}"#
        ),
    );
}

#[test]
fn an_update_carries_the_event_reference_and_the_wall_clock_edit() {
    // The minimal edit renames one event; every field it does not touch crosses no key.
    round_trips(
        &Intent::Calendar(CalendarIntent::UpdateEvent {
            event: event(),
            edit: EventEdit {
                title: Some("Renamed".to_owned()),
                ..EventEdit::default()
            },
        }),
        concat!(
            r#"{"calendar":{"update_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"},"edit":{"title":"Renamed"}}}}"#
        ),
    );
    // The full edit: new clocks, notes cleared (the empty string, not an absent key), a
    // location, the rule cleared, and the occurrence the edit is scoped to.
    round_trips(
        &Intent::Calendar(CalendarIntent::UpdateEvent {
            event: event(),
            edit: EventEdit {
                title: None,
                start: Some(local("2026-10-20T09:30:00")),
                end: Some(local("2026-10-20T10:00:00")),
                notes: Some(String::new()),
                location: Some("Room B".to_owned()),
                recurrence: Some(RecurrenceChange::Clear),
                occurrence: Some(local("2026-10-20T09:00:00")),
                times_from_occurrence: None,
            },
        }),
        concat!(
            r#"{"calendar":{"update_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"},"#,
            r#""edit":{"start":"2026-10-20T09:30:00","end":"2026-10-20T10:00:00","#,
            r#""notes":"","location":"Room B","recurrence":"clear","#,
            r#""occurrence":"2026-10-20T09:00:00"}}}}"#
        ),
    );
    // A series edit seeded from one occurrence names where its clocks were read from.
    round_trips(
        &Intent::Calendar(CalendarIntent::UpdateEvent {
            event: event(),
            edit: EventEdit {
                start: Some(local("2026-10-20T09:30:00")),
                end: Some(local("2026-10-20T10:00:00")),
                times_from_occurrence: Some(local("2026-10-20T09:00:00")),
                ..EventEdit::default()
            },
        }),
        concat!(
            r#"{"calendar":{"update_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"},"#,
            r#""edit":{"start":"2026-10-20T09:30:00","end":"2026-10-20T10:00:00","#,
            r#""times_from_occurrence":"2026-10-20T09:00:00"}}}}"#
        ),
    );
}

#[test]
fn a_move_carries_the_drag_in_the_grid_s_own_currency() {
    // A whole-block drag to the next day, half an hour earlier: one occurrence of a series.
    round_trips(
        &Intent::Calendar(CalendarIntent::MoveEvent {
            event: event(),
            drag: EventDrag {
                edge: EventEdge::Whole,
                days: 1,
                minutes: -30,
                occurrence: Some(local("2026-10-20T09:00:00")),
            },
        }),
        concat!(
            r#"{"calendar":{"move_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"},"#,
            r#""drag":{"edge":"whole","days":1,"minutes":-30,"#,
            r#""occurrence":"2026-10-20T09:00:00"}}}}"#
        ),
    );
    // A resize of the end edge, on the whole series: no occurrence key crosses.
    round_trips(
        &Intent::Calendar(CalendarIntent::MoveEvent {
            event: event(),
            drag: EventDrag {
                edge: EventEdge::End,
                days: 0,
                minutes: 45,
                occurrence: None,
            },
        }),
        concat!(
            r#"{"calendar":{"move_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"},"#,
            r#""drag":{"edge":"end","days":0,"minutes":45}}}}"#
        ),
    );
}

#[test]
fn a_delete_names_the_event_and_optionally_the_one_occurrence() {
    round_trips(
        &Intent::Calendar(CalendarIntent::DeleteEvent {
            event: event(),
            occurrence: None,
        }),
        concat!(
            r#"{"calendar":{"delete_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"}}}}"#
        ),
    );
    round_trips(
        &Intent::Calendar(CalendarIntent::DeleteEvent {
            event: event(),
            occurrence: Some(local("2026-10-20T09:00:00")),
        }),
        concat!(
            r#"{"calendar":{"delete_event":{"event":{"account":"work","#,
            r#""key":"/dav/cal/work/default/evt-1.ics"},"#,
            r#""occurrence":"2026-10-20T09:00:00"}}}"#
        ),
    );
}

#[test]
fn an_invitation_answer_names_the_message_and_the_answer() {
    // The plain accept: no note, the organiser told (the RFC 5546 default).
    round_trips(
        &Intent::Calendar(CalendarIntent::RespondToInvitation {
            message: MessageRef::from_parts("work", "imap:v1:u7@INBOX".to_owned()).unwrap(),
            response: RsvpAnswer::Accepted,
            comment: None,
            notify_organizer: true,
        }),
        concat!(
            r#"{"calendar":{"respond_to_invitation":{"message":{"account":"work","#,
            r#""key":"imap:v1:u7@INBOX"},"response":"accepted","notify_organizer":true}}}"#
        ),
    );
    // A quiet decline with a note: the organiser is not told.
    round_trips(
        &Intent::Calendar(CalendarIntent::RespondToInvitation {
            message: MessageRef::from_parts("work", "imap:v1:u7@INBOX".to_owned()).unwrap(),
            response: RsvpAnswer::Declined,
            comment: Some("Clashes with the offsite".to_owned()),
            notify_organizer: false,
        }),
        concat!(
            r#"{"calendar":{"respond_to_invitation":{"message":{"account":"work","#,
            r#""key":"imap:v1:u7@INBOX"},"response":"declined","#,
            r#""comment":"Clashes with the offsite","notify_organizer":false}}}"#
        ),
    );
    // The third answer is its own word.
    round_trips(
        &Intent::Calendar(CalendarIntent::RespondToInvitation {
            message: MessageRef::from_parts("work", "imap:v1:u7@INBOX".to_owned()).unwrap(),
            response: RsvpAnswer::Tentative,
            comment: None,
            notify_organizer: true,
        }),
        concat!(
            r#"{"calendar":{"respond_to_invitation":{"message":{"account":"work","#,
            r#""key":"imap:v1:u7@INBOX"},"response":"tentative","notify_organizer":true}}}"#
        ),
    );
}

#[test]
fn the_calendar_surface_crosses_as_its_renamed_word() {
    assert_eq!(
        serde_json::to_string(&Surface::Calendar).unwrap(),
        r#""calendar""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""calendar""#).unwrap(),
        Surface::Calendar
    );
}

#[test]
fn a_malformed_calendar_or_event_reference_is_refused_on_the_wire() {
    // Both halves validate on the way in, exactly as a message reference's do: a blank id
    // or key cannot ride an intent into the core and route a calendar write by half a
    // reference.
    let blank_account =
        r#"{"calendar":{"delete_event":{"event":{"account":"","key":"evt-1.ics"}}}}"#;
    assert!(serde_json::from_str::<Intent>(blank_account).is_err());
    let blank_key = r#"{"calendar":{"delete_event":{"event":{"account":"work","key":""}}}}"#;
    assert!(serde_json::from_str::<Intent>(blank_key).is_err());
    let blank_calendar =
        r#"{"calendar":{"set_visible_calendars":{"ids":[{"account":"","key":"cal-1"}]}}}"#;
    assert!(serde_json::from_str::<Intent>(blank_calendar).is_err());
    assert!(EventRef::from_parts("", "evt-1.ics".to_owned()).is_none());
    assert!(CalendarRef::from_parts("work", String::new()).is_none());
    assert_eq!(
        account("work").as_str(),
        "work",
        "the account half reads back as the host gave it"
    );
}
