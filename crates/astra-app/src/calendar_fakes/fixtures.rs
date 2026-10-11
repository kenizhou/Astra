// SPDX-License-Identifier: GPL-3.0-only
//! The diary and tree builders every calendar test starts from: calendars,
//! events in the three time forms, the recurring series, the meeting with its
//! organiser and attendee, and the invitation message that carries one.

use engine_api::{
    Calendar, CalendarDate, CalendarDateTime, CalendarId, Duration, Event, EventId, Frequency,
    Mailbox, MailboxRole, Message, MessageIdHeader, Participant, ParticipantRole, Recurrence,
    RecurrenceRule, TimeZoneId, Uid, UtcDateTime,
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
