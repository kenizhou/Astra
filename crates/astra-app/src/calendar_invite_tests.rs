// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the invitation machine: the two-condition gate, the staleness
//! downgrade, the matched alias, the route decision from the capabilities, the
//! `METHOD:REPLY` document, and the whole answer over the fakes on each route
//! (the server that schedules, the one that cannot, and the one that says it
//! could not deliver).

use std::sync::{Arc, Mutex};

use engine_api::{InboundScheduling, ScheduleMethod, SchedulingMessage};

use super::{classify, matched_attendee, reply_route, supersede, InvitationKind};
use crate::calendar::imip::{reply_document, ImipReply};
use crate::calendar_fakes::{
    account, calendar_app, draft_method, meeting, partstat_of, response_word, utc,
};
use crate::{
    calendar_fakes::CalendarFake, CalendarIntent, Intent, MessageRef, RsvpAnswer, Surface,
};

/// The parsed invitation, the shape every gate test reads.
fn scheduling() -> InboundScheduling {
    InboundScheduling {
        message: SchedulingMessage::new(
            ScheduleMethod::Request,
            meeting("imip:meeting-1", "info@test.local"),
            utc("2026-07-01T08:00:00Z"),
        ),
        media_type: "text/calendar".to_owned(),
        from_inline_body: true,
        delivery_recipients: vec!["info@test.local".to_owned()],
    }
}

#[test]
fn the_gate_offers_an_answer_only_where_one_is_owed() {
    assert_eq!(
        classify(&ScheduleMethod::Request, true),
        Some(InvitationKind::Rsvp)
    );
    assert_eq!(
        classify(&ScheduleMethod::Request, false),
        Some(InvitationKind::Informational)
    );
    assert_eq!(
        classify(&ScheduleMethod::Cancel, true),
        Some(InvitationKind::Cancelled)
    );
    // A `PUBLISH` is informational with no organiser waiting; a `REPLY` and a
    // `COUNTER` produce no card at all.
    assert_eq!(classify(&ScheduleMethod::Publish, true), None);
    assert_eq!(classify(&ScheduleMethod::Reply, true), None);
    assert_eq!(classify(&ScheduleMethod::Counter, true), None);
}

#[test]
fn a_stored_higher_sequence_downgrades_the_answerable_invitation() {
    let mut newer = meeting("evt-1", "info@test.local");
    newer.sequence = 1;
    assert_eq!(
        supersede(InvitationKind::Rsvp, 0, Some(&newer)),
        InvitationKind::Superseded
    );
    // An equal sequence is not a supersession: idempotent re-delivery of the
    // same invitation still offers the answer it always did.
    let same = meeting("evt-1", "info@test.local");
    assert_eq!(
        supersede(InvitationKind::Rsvp, 0, Some(&same)),
        InvitationKind::Rsvp
    );
    assert_eq!(
        supersede(InvitationKind::Rsvp, 0, None),
        InvitationKind::Rsvp
    );
}

#[test]
fn the_matched_attendee_is_the_alias_the_message_was_delivered_to() {
    let found = scheduling();
    assert_eq!(matched_attendee(&found).as_deref(), Some("info@test.local"));
    // Delivered to a list the attendee is not on: somebody else's meeting.
    let mut forwarded = scheduling();
    forwarded.delivery_recipients = vec!["everyone@test.local".to_owned()];
    assert_eq!(matched_attendee(&forwarded), None);
}

#[test]
fn the_route_comes_from_the_two_capabilities_that_decide_it() {
    use super::ReplyRoute;
    assert_eq!(reply_route(true, true, false), ReplyRoute::Server);
    assert_eq!(reply_route(true, false, true), ReplyRoute::ClientImip);
    assert_eq!(reply_route(false, false, true), ReplyRoute::ClientImip);
    assert_eq!(reply_route(true, false, false), ReplyRoute::None);
    assert_eq!(reply_route(false, true, false), ReplyRoute::None);
    // The arm worth reading twice: a server that schedules over a calendar we
    // cannot write an answer into delivers nothing (the thing it schedules on
    // is the write we cannot make), so no route at all.
    assert_eq!(reply_route(false, true, true), ReplyRoute::None);
}

#[test]
fn the_reply_document_carries_the_organiser_and_only_the_replying_attendee() {
    let document = reply_document(&ImipReply {
        uid: "meeting-1@test.local",
        sequence: 0,
        organizer: "mailto:boss@test.local",
        attendee: "info@test.local",
        attendee_name: Some("Info"),
        status: RsvpAnswer::Accepted,
        starts_at: utc("2026-07-30T14:30:00Z"),
        recurrence_id: None,
        stamp: utc("2026-10-19T10:00:00Z"),
        comment: Some("See you then"),
    });
    assert_eq!(
        document,
        concat!(
            "BEGIN:VCALENDAR\r\n",
            "PRODID:-//Astra Mail//EN\r\n",
            "VERSION:2.0\r\n",
            "METHOD:REPLY\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:meeting-1@test.local\r\n",
            "DTSTAMP:20261019T100000Z\r\n",
            "DTSTART:20260730T143000Z\r\n",
            "SEQUENCE:0\r\n",
            "ORGANIZER:mailto:boss@test.local\r\n",
            "ATTENDEE;CN=\"Info\";PARTSTAT=ACCEPTED:mailto:info@test.local\r\n",
            "COMMENT:See you then\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        )
    );
    assert_eq!(
        document.matches("ATTENDEE").count(),
        1,
        "exactly one attendee line: {document}"
    );
}

#[test]
fn the_reply_document_escapes_and_names_the_instance_it_answers() {
    let document = reply_document(&ImipReply {
        uid: "meeting, semi;colon@test.local",
        sequence: 2,
        organizer: "boss@test.local",
        attendee: "info@test.local",
        attendee_name: None,
        status: RsvpAnswer::Declined,
        starts_at: utc("2026-07-30T14:30:00Z"),
        recurrence_id: Some(utc("2026-08-06T14:30:00Z")),
        stamp: utc("2026-10-19T10:00:00Z"),
        comment: None,
    });
    assert!(document.contains("UID:meeting\\, semi\\;colon@test.local"));
    assert!(document.contains("RECURRENCE-ID:20260806T143000Z"));
    assert!(document.contains("PARTSTAT=DECLINED"));
    assert!(document.contains("ORGANIZER:mailto:boss@test.local"));
}

/// Boots the app over `fake` with the invitation in the store and the
/// meeting's week in the calendar window, the shared prefix of every answer
/// test.
async fn answered_app(
    fake: CalendarFake,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> crate::App<CalendarFake> {
    let app = calendar_app(vec![account("work", fake)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-07-27T00:00:00Z"),
        days: 7,
        view: None,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app
}

/// The `RespondToInvitation` intent over the invitation message.
fn respond(notify: bool) -> Intent {
    Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Accepted,
        comment: None,
        notify_organizer: notify,
    })
}

#[tokio::test]
async fn on_a_scheduling_server_the_answer_is_the_write_and_no_email() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(meeting("evt-1", "info@test.local"));
    let rsvps = Arc::clone(&fake.rsvps);
    let submits = Arc::clone(&fake.submits);
    let diary = Arc::clone(&fake.events);
    let app = answered_app(fake, &surfaces).await;

    app.dispatch(respond(true)).await.unwrap();

    assert_eq!(rsvps.lock().unwrap().len(), 1, "the answer was written");
    let (_, rsvp) = &rsvps.lock().unwrap()[0];
    assert_eq!(
        rsvp.attendee, "info@test.local",
        "the address the invitation matched, not the account's primary"
    );
    assert_eq!(response_word(rsvp), "accepted");
    assert_eq!(
        partstat_of(&diary.lock().unwrap()[0], "info@test.local"),
        "accepted",
        "the server's copy carries the answer"
    );
    assert!(
        submits.lock().unwrap().is_empty(),
        "a scheduling server is trusted to deliver; a second reply of our own \
         would reach the organiser twice"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::Calendar),
        "the calendar surface published the write's outcome"
    );
}

#[tokio::test]
async fn when_nothing_schedules_the_answer_travels_as_an_imip_email() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .without_scheduling()
        .holding(meeting("evt-1", "info@test.local"));
    let rsvps = Arc::clone(&fake.rsvps);
    let submits = Arc::clone(&fake.submits);
    let app = answered_app(fake, &surfaces).await;

    app.dispatch(respond(true)).await.unwrap();

    assert_eq!(rsvps.lock().unwrap().len(), 1, "the diary still records it");
    let sent = submits.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "the reply was mailed");
    let draft = &sent[0];
    assert_eq!(draft.from.email, "info@test.local");
    assert_eq!(draft.to.len(), 1);
    assert_eq!(draft.to[0].email, "boss@test.local");
    assert_eq!(draft_method(draft), Some(ScheduleMethod::Reply));
    let calendar = draft.calendar.as_ref().unwrap();
    assert!(calendar.ical.contains("METHOD:REPLY"));
    assert!(calendar.ical.contains("PARTSTAT=ACCEPTED"));
    assert!(calendar.ical.contains("mailto:info@test.local"));
    // The reply threads onto the invitation it answers.
    assert_eq!(
        draft
            .in_reply_to
            .as_ref()
            .map(engine_api::MessageIdHeader::as_str),
        Some("<invite-1@test.local>")
    );
}

#[tokio::test]
async fn a_silent_server_is_not_a_failed_one_but_a_saying_one_prompts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .failing_reply_delivery()
        .holding(meeting("evt-1", "info@test.local"));
    let submits = Arc::clone(&fake.submits);
    let app = answered_app(fake, &surfaces).await;

    app.dispatch(respond(true)).await.unwrap();

    let prompts = &app.calendar().reply_prompts;
    assert_eq!(prompts.len(), 1, "the broken promise is recorded");
    assert_eq!(prompts[0].status_code, "5.2");
    assert_eq!(prompts[0].summary, "Sprint planning");
    assert_eq!(prompts[0].organizer, "boss@test.local");
    assert_eq!(
        prompts[0].response,
        astra_viewmodel::Participation::Accepted
    );
    assert!(
        submits.lock().unwrap().is_empty(),
        "sending mail as the user is the user's decision, never a silent repair"
    );

    // The prompt survives a refresh: the organiser still has not heard, so the
    // decision stays on offer until the invitation is answered again.
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert_eq!(
        app.calendar().reply_prompts.len(),
        1,
        "a re-read clears nothing the organiser is still waiting on"
    );
}

#[tokio::test]
async fn a_stale_invitation_offers_no_answer() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut newer = meeting("evt-1", "info@test.local");
    newer.sequence = 1;
    let fake = CalendarFake::with_calendar_and_mail().holding(newer);
    let rsvps = Arc::clone(&fake.rsvps);
    let app = answered_app(fake, &surfaces).await;

    app.dispatch(respond(true)).await.unwrap();

    assert!(
        rsvps.lock().unwrap().is_empty(),
        "the calendar holds a newer revision; answering the old copy agrees to \
         times that are no longer the meeting's"
    );
}

#[tokio::test]
async fn a_meeting_that_never_left_the_mailbox_is_stored_then_answered() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .storing_document("meeting-1@test.local", meeting("stored", "info@test.local"));
    let puts = Arc::clone(&fake.puts);
    let rsvps = Arc::clone(&fake.rsvps);
    let diary = Arc::clone(&fake.events);
    let app = answered_app(fake, &surfaces).await;

    app.dispatch(respond(true)).await.unwrap();

    assert_eq!(puts.lock().unwrap().len(), 1, "the invitation was stored");
    let write = &puts.lock().unwrap()[0];
    assert!(
        matches!(write.guard, engine_api::WritePrecondition::IfAbsent),
        "the create is guarded: a copy the server deposited is found, not overwritten"
    );
    assert_eq!(rsvps.lock().unwrap().len(), 1);
    let (_, rsvp) = &rsvps.lock().unwrap()[0];
    assert_eq!(rsvp.attendee, "info@test.local");
    assert_eq!(
        partstat_of(&diary.lock().unwrap()[0], "info@test.local"),
        "accepted"
    );
}

#[tokio::test]
async fn a_note_the_transport_cannot_carry_is_refused_not_dropped() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .without_rsvp_notes()
        .holding(meeting("evt-1", "info@test.local"));
    let rsvps = Arc::clone(&fake.rsvps);
    let app = answered_app(fake, &surfaces).await;

    app.dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Tentative,
        comment: Some("Hopefully".to_owned()),
        notify_organizer: true,
    }))
    .await
    .unwrap();

    assert!(
        rsvps.lock().unwrap().is_empty(),
        "a note that silently goes nowhere is worse than a note refused"
    );
}
