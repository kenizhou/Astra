// SPDX-License-Identifier: GPL-3.0-only
//! The diary and tree builders every calendar test starts from: calendars,
//! events in the three time forms, the recurring series, the meeting with its
//! organiser and attendee, and the invitation message that carries one.

use std::num::NonZeroU32;

use engine_api::{
    Calendar, CalendarDate, CalendarDateTime, CalendarId, Duration, Event, EventId, Frequency,
    Mailbox, MailboxRole, Message, MessageIdHeader, Participant, ParticipantRole,
    ParticipationStatus, Recurrence, RecurrenceRule, TimeZoneId, Uid, UtcDateTime,
};
use engine_core::{ids::MessageId, membership::Memberships};

/// A wall clock, the way every fixture names one.
pub(crate) fn local(value: &str) -> engine_api::LocalDateTime {
    value.parse().unwrap()
}

/// A UTC instant, the way the range fixtures name one.
pub(crate) fn utc(value: &str) -> UtcDateTime {
    value.parse().unwrap()
}

/// The role-bearing Inbox every account's tree leads with.
pub(crate) fn inbox() -> Mailbox {
    let mut mailbox = Mailbox::new(engine_api::MailboxId::try_from("inbox").unwrap(), "Inbox");
    mailbox.role = Some(MailboxRole::Inbox);
    mailbox
}

/// A writable default calendar keyed `key` called `name`.
pub(crate) fn calendar(key: &str, name: &str) -> Calendar {
    let mut calendar = Calendar::new(CalendarId::try_from(key).unwrap(), name);
    calendar.is_default = true;
    calendar
}

/// A read-only calendar keyed `key`: the write gate's refusal case.
pub(crate) fn readonly(key: &str, name: &str) -> Calendar {
    let mut calendar = Calendar::new(CalendarId::try_from(key).unwrap(), name);
    calendar.access = engine_core::calendar::CalendarAccess::reader();
    calendar
}

/// A one-off zoned event `key` titled `title`, starting at `start` in
/// Amsterdam, running an hour.
pub(crate) fn zoned_event(key: &str, title: &str, start: &str) -> Event {
    let mut event = Event::new(
        EventId::try_from(key).unwrap(),
        Uid::new(format!("{key}@test.local")).unwrap(),
        Memberships::of_one(CalendarId::try_from("cal-1").unwrap()),
        CalendarDateTime::Zoned {
            local: local(start),
            zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
        },
    );
    event.title = title.to_owned();
    event.duration = Duration::from_parts(0, 0, 1, 0, 0, 0).unwrap();
    event
}

/// A weekly zoned event `key` titled `title`, starting at `start`.
pub(crate) fn weekly_event(key: &str, title: &str, start: &str) -> Event {
    let mut event = zoned_event(key, title, start);
    event.recurrence = Some(Recurrence::from_rule(RecurrenceRule::new(
        Frequency::Weekly,
    )));
    event
}

/// A weekly zoned series carrying one overridden occurrence (the Tuesday a
/// week after the fixtures' Monday start, excluded the way an occurrence
/// delete writes one): the diary shape the series warning reads. The override
/// sits on a later occurrence so the master's own start still materializes.
pub(crate) fn overridden_weekly_event(key: &str, title: &str, start: &str) -> Event {
    let mut event = weekly_event(key, title, start);
    if let Some(recurrence) = &mut event.recurrence {
        recurrence.overrides.insert(
            local("2026-11-03T09:00:00"),
            engine_api::RecurrenceOverride::Excluded,
        );
    }
    event
}

/// An all-day event `key` titled `title` on `date`, one day long.
pub(crate) fn all_day_event(key: &str, title: &str, date: &str) -> Event {
    let mut event = Event::new(
        EventId::try_from(key).unwrap(),
        Uid::new(format!("{key}@test.local")).unwrap(),
        Memberships::of_one(CalendarId::try_from("cal-1").unwrap()),
        CalendarDateTime::Date(date.parse::<CalendarDate>().unwrap()),
    );
    event.title = title.to_owned();
    event.duration = Duration::from_parts(0, 1, 0, 0, 0, 0).unwrap();
    event
}

/// The invitation's own meeting as a stored event: the organiser's copy a
/// server deposited, attendee `attendee` still needs-action.
pub(crate) fn meeting(key: &str, attendee: &str) -> Event {
    let mut event = Event::new(
        EventId::try_from(key).unwrap(),
        Uid::new("meeting-1@test.local").unwrap(),
        Memberships::of_one(CalendarId::try_from("cal-1").unwrap()),
        CalendarDateTime::Zoned {
            local: local("2026-07-30T16:30:00"),
            zone: TimeZoneId::iana("Europe/Berlin").unwrap(),
        },
    );
    event.title = "Sprint planning".to_owned();
    event.duration = Duration::from_parts(0, 0, 1, 0, 0, 0).unwrap();
    let mut organizer = Participant::attendee("boss@test.local");
    organizer.roles.insert(ParticipantRole::Owner);
    event.participants = vec![organizer, Participant::attendee(attendee)];
    event
}

/// One participant named `name` at `address`, with `roles` beside the plain
/// attendee's and the `status` they answered.
fn seated(
    name: Option<&str>,
    address: &str,
    roles: &[ParticipantRole],
    status: ParticipationStatus,
) -> Participant {
    let mut participant = Participant::attendee(address);
    participant.name = name.map(str::to_owned);
    for role in roles {
        participant.roles.insert(role.clone());
    }
    participant.participation_status = status;
    participant
}

/// The meeting whose roll-call exercises every word: the organiser stored
/// last (the roll-call must move the owner line first), a chair, an optional
/// attendee, an informational address with no name, a plain attendee, and a
/// line with neither a name nor an address, which renders nothing at all.
pub(crate) fn roll_call_meeting(key: &str) -> Event {
    let mut event = zoned_event(key, "Quarterly review", "2026-10-21T15:00:00");
    let owner = ParticipantRole::Owner;
    let chair = ParticipantRole::Chair;
    let optional = ParticipantRole::Optional;
    let informational = ParticipantRole::Informational;
    event.participants = vec![
        seated(
            Some("Kim"),
            "kim@test.local",
            &[],
            ParticipationStatus::NeedsAction,
        ),
        seated(
            Some("Boss"),
            "boss@test.local",
            &[owner],
            ParticipationStatus::Accepted,
        ),
        seated(
            Some("Ada"),
            "ada@test.local",
            &[chair],
            ParticipationStatus::Tentative,
        ),
        seated(
            Some("Sam"),
            "sam@test.local",
            &[optional],
            ParticipationStatus::Declined,
        ),
        seated(
            None,
            "digest@test.local",
            &[informational],
            ParticipationStatus::NeedsAction,
        ),
        Participant {
            name: None,
            email: None,
            ..Participant::attendee("unreachable@test.local")
        },
    ];
    event
}

/// The meeting of `attendees` plain attendees beside the organiser: the
/// roll-call cap's fold case.
pub(crate) fn crowded_meeting(key: &str, attendees: usize) -> Event {
    let mut event = zoned_event(key, "All hands", "2026-10-21T10:00:00");
    let mut organizer = Participant::attendee("boss@test.local");
    organizer.name = Some("Boss".to_owned());
    organizer.roles.insert(ParticipantRole::Owner);
    organizer.participation_status = ParticipationStatus::Accepted;
    event.participants = Vec::with_capacity(attendees + 1);
    event.participants.push(organizer);
    for seat in 0..attendees {
        event
            .participants
            .push(Participant::attendee(format!("guest{seat}@test.local")));
    }
    event
}

/// The series the read-back words in full: every third week on Mondays and
/// Wednesdays, ten instances.
pub(crate) fn every_third_week_series(key: &str) -> Event {
    let mut event = zoned_event(key, "Third-week sync", "2026-10-19T09:00:00");
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.interval = NonZeroU32::new(3).unwrap();
    rule.by_day = vec![
        engine_api::NDay {
            day: engine_api::Weekday::We,
            nth_of_period: None,
        },
        engine_api::NDay {
            day: engine_api::Weekday::Mo,
            nth_of_period: None,
        },
    ];
    rule.bound = engine_api::RecurrenceBound::Count(NonZeroU32::new(10).unwrap());
    event.recurrence = Some(Recurrence::from_rule(rule));
    event
}

/// The series whose rule sits outside the wording vocabulary (a by-month-day
/// pattern): the read-back falls back to the bare interval and frequency.
pub(crate) fn exotic_series(key: &str) -> Event {
    let mut event = zoned_event(key, "Payday", "2026-10-15T09:00:00");
    let mut rule = RecurrenceRule::new(Frequency::Monthly);
    rule.by_month_day = vec![15];
    event.recurrence = Some(Recurrence::from_rule(rule));
    event
}

/// The invitation message, in the Inbox, from the organiser.
pub(crate) fn invitation_message(key: &str) -> Message {
    let mut message = Message::new(
        MessageId::try_from(key).unwrap(),
        Memberships::of_one(engine_api::MailboxId::try_from("inbox").unwrap()),
    );
    message.envelope.subject = Some("Sprint planning".to_owned());
    message
        .envelope
        .from
        .push(engine_api::EmailAddress::named("Boss", "boss@test.local"));
    message
        .envelope
        .message_id
        .push(MessageIdHeader::new("<invite-1@test.local>").unwrap());
    message.received_at = Some(utc("2026-07-01T08:00:00Z"));
    message
}
