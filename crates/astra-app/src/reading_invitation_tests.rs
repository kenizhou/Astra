// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading pane's invitation block: each kind the two-condition
//! gate classifies, the participation the stored copy carries, the stale flag
//! a higher stored `SEQUENCE` raises, and the plain message that yields no
//! block at all. The fixtures are the calendar fake's own invitation and its
//! store, so the card and the answer path run over one meeting.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{InvitationKind, Participation};

use crate::calendar_fakes::{
    account as calendar_account, calendar_app, meeting, utc, CalendarFake,
};
use crate::{CalendarIntent, Intent, MessageRef, RsvpAnswer, Surface};

/// A `METHOD:CANCEL` body for the fixture meeting, delivered to the alias the
/// REQUEST fixtures are delivered to.
const CANCEL_SOURCE: &[u8] = b"From: boss@test.local\r\n\
    To: everyone@test.local\r\n\
    Delivered-To: info@test.local\r\n\
    Subject: Cancelled: Sprint planning\r\n\
    Message-ID: <cancel-1@test.local>\r\n\
    Content-Type: text/calendar; charset=\"utf-8\"; method=CANCEL\r\n\r\n\
    BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:CANCEL\r\nBEGIN:VEVENT\r\n\
    UID:meeting-1@test.local\r\nDTSTAMP:20260801T080000Z\r\n\
    DTSTART;TZID=Europe/Berlin:20260730T163000\r\n\
    SUMMARY:Sprint planning\r\nSEQUENCE:0\r\n\
    ORGANIZER;CN=Boss:mailto:boss@test.local\r\n\
    ATTENDEE;CN=Info;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:info@test.local\r\n\
    END:VEVENT\r\nEND:VCALENDAR\r\n";

/// A `METHOD:REQUEST` body delivered to somebody else entirely: the meeting a
/// forward of an invitation is, informational for this account.
const FORWARDED_SOURCE: &[u8] = b"From: boss@test.local\r\n\
    To: someone-else@test.local\r\n\
    Delivered-To: someone-else@test.local\r\n\
    Subject: Sprint planning\r\n\
    Message-ID: <invite-2@test.local>\r\n\
    Content-Type: text/calendar; charset=\"utf-8\"; method=REQUEST\r\n\r\n\
    BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n\
    UID:meeting-2@test.local\r\nDTSTAMP:20260701T080000Z\r\n\
    DTSTART;TZID=Europe/Berlin:20260730T163000\r\n\
    DTEND;TZID=Europe/Berlin:20260730T173000\r\n\
    SUMMARY:Sprint planning\r\nSEQUENCE:0\r\n\
    ORGANIZER;CN=Boss:mailto:boss@test.local\r\n\
    ATTENDEE;CN=Info;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:info@test.local\r\n\
    END:VEVENT\r\nEND:VCALENDAR\r\n";

/// A plain message in the Inbox beside the invitations, carrying no calendar
/// part at all.
fn plain_message(key: &str) -> engine_api::Message {
    let mut message = engine_api::Message::new(
        engine_core::ids::MessageId::try_from(key).unwrap(),
        engine_core::membership::Memberships::of_one(
            engine_api::MailboxId::try_from("inbox").unwrap(),
        ),
    );
    message.envelope.subject = Some("Hello".to_owned());
    message.received_at = Some(utc("2026-07-01T08:00:00Z"));
    message
}

/// Adds `key` to the fake's Inbox with `source` as its bytes, so an open can
/// read a second invitation shape beside the fixture REQUEST.
fn holding_source(mut fake: CalendarFake, key: &str, source: &[u8]) -> CalendarFake {
    fake.messages.push(plain_message(key));
    fake.sources.insert(key.to_owned(), source.to_vec());
    fake
}

/// Boots the calendar fake's app with the meeting's week expanded and `key`
/// open in the reading pane: the shared prefix of every invitation-card test.
async fn opened_invitation(
    fake: CalendarFake,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    key: &str,
) -> crate::App<CalendarFake> {
    let app = calendar_app(vec![calendar_account("work", fake)], surfaces);
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
    app.dispatch(Intent::OpenMessage {
        message: MessageRef::from_parts("work", key.to_owned()).unwrap(),
    })
    .await
    .unwrap();
    app
}

#[tokio::test]
async fn an_answerable_invitation_offers_the_rsvp_with_the_hold_standing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(meeting("evt-1", "info@test.local"));
    let app = opened_invitation(fake, &surfaces, "invite-1").await;

    let snapshot = app.reading().expect("the open invitation");
    assert_eq!(
        snapshot.invitation,
        Some(astra_viewmodel::InvitationBlock {
            kind: InvitationKind::Rsvp,
            response: Participation::NeedsAction,
            stale: false,
            summary: "Sprint planning".to_owned(),
            organizer: "boss@test.local".to_owned(),
        }),
        "the card names the meeting, the organiser, and the unanswered hold"
    );
}

#[tokio::test]
async fn an_answered_meeting_carries_the_participation_the_diary_holds() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(meeting("evt-1", "info@test.local"));
    let app = opened_invitation(fake, &surfaces, "invite-1").await;
    app.dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Accepted,
        comment: None,
        notify_organizer: true,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::OpenMessage {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
    })
    .await
    .unwrap();

    let block = app.reading().expect("still open").invitation;
    assert_eq!(
        block.map(|block| (block.kind, block.response)),
        Some((InvitationKind::Rsvp, Participation::Accepted)),
        "the answer the diary holds rides the card on the next open"
    );
}

#[tokio::test]
async fn a_stored_newer_revision_marks_the_copy_stale_and_supersedes_the_answer() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut newer = meeting("evt-1", "info@test.local");
    newer.sequence = 1;
    let fake = CalendarFake::with_calendar_and_mail().holding(newer);
    let app = opened_invitation(fake, &surfaces, "invite-1").await;

    let block = app.reading().expect("the open invitation").invitation;
    assert_eq!(
        block.map(|block| (block.kind, block.stale)),
        Some((InvitationKind::Superseded, true)),
        "the calendar holds a newer revision; the copy on screen is out of date"
    );
}

#[tokio::test]
async fn a_cancelled_meeting_sentences_without_an_answer() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = holding_source(
        CalendarFake::with_calendar_and_mail(),
        "cancel-1",
        CANCEL_SOURCE,
    );
    let app = opened_invitation(fake, &surfaces, "cancel-1").await;

    let block = app.reading().expect("the open cancellation").invitation;
    assert_eq!(
        block.map(|block| (block.kind, block.stale)),
        Some((InvitationKind::Cancelled, false))
    );
}

#[tokio::test]
async fn somebody_elses_meeting_is_informational_for_this_account() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = holding_source(
        CalendarFake::with_calendar_and_mail(),
        "invite-2",
        FORWARDED_SOURCE,
    );
    let app = opened_invitation(fake, &surfaces, "invite-2").await;

    let block = app.reading().expect("the open forward").invitation;
    assert_eq!(
        block.map(|block| block.kind),
        Some(InvitationKind::Informational),
        "a REQUEST this account was not delivered to is not this account's answer to give"
    );
}

#[tokio::test]
async fn a_message_that_is_no_invitation_carries_no_block() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = holding_source(
        CalendarFake::with_calendar_and_mail(),
        "plain-1",
        b"Subject: Hello\r\n\r\nJust words, no calendar part.\r\n",
    );
    let app = opened_invitation(fake, &surfaces, "plain-1").await;

    assert_eq!(
        app.reading().expect("the plain body").invitation,
        None,
        "a plain message is no invitation, whatever else it carries"
    );
}

/// A `METHOD:REQUEST` payload as iMIP carries it, for the boundary tests: the
/// calendar part itself, ready to embed in whatever MIME shape the test wraps
/// it in.
const REQUEST_PAYLOAD: &[u8] = b"BEGIN:VCALENDAR\r\n\
    VERSION:2.0\r\n\
    METHOD:REQUEST\r\n\
    BEGIN:VEVENT\r\n\
    UID:gmail-1@test.local\r\n\
    DTSTAMP:20260801T080000Z\r\n\
    DTSTART;TZID=Europe/Berlin:20260730T163000\r\n\
    DTEND;TZID=Europe/Berlin:20260730T173000\r\n\
    SUMMARY:Sprint planning\r\n\
    SEQUENCE:0\r\n\
    ORGANIZER;CN=Boss:mailto:boss@test.local\r\n\
    ATTENDEE;CN=Info;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:info@test.local\r\n\
    END:VEVENT\r\n\
    END:VCALENDAR\r\n";

/// A `METHOD:PUBLISH` calendar file, the shape a genuinely attached `.ics`
/// carries: a calendar to keep, not a meeting to answer.
const PUBLISH_PAYLOAD: &[u8] = b"BEGIN:VCALENDAR\r\n\
    VERSION:2.0\r\n\
    METHOD:PUBLISH\r\n\
    BEGIN:VEVENT\r\n\
    UID:season-1@test.local\r\n\
    DTSTAMP:20260801T080000Z\r\n\
    DTSTART;TZID=Europe/Berlin:20260901T090000\r\n\
    DTEND;TZID=Europe/Berlin:20260901T100000\r\n\
    SUMMARY:Season kickoff\r\n\
    SEQUENCE:0\r\n\
    END:VEVENT\r\n\
    END:VCALENDAR\r\n";

/// The Gmail shape: the invitation as an alternative body part, plus a
/// duplicate copy the sender explicitly attached as `invite.ics`.
const GMAIL_INVITATION: &[u8] = b"From: boss@test.local\r\n\
    To: everyone@test.local\r\n\
    Delivered-To: info@test.local\r\n\
    Subject: Sprint planning\r\n\
    Message-ID: <gmail-1@test.local>\r\n\
    Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
    --mix\r\n\
    Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n\
    --alt\r\n\
    Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
    When: 30 July\r\n\
    --alt\r\n\
    Content-Type: text/calendar; charset=\"utf-8\"; method=REQUEST\r\n\r\n";

/// The dispositioned duplicate copy of the same invitation.
const GMAIL_ATTACHMENT: &[u8] = b"--mix\r\n\
    Content-Type: text/calendar; name=\"invite.ics\"\r\n\
    Content-Disposition: attachment; filename=\"invite.ics\"\r\n\r\n";

/// The ordinary attached calendar file: a plain message and a `.ics` the
/// sender meant to send, carrying no invitation of its own.
const ATTACHED_CALENDAR: &[u8] = b"From: coach@test.local\r\n\
    To: info@test.local\r\n\
    Subject: Season dates\r\n\
    Message-ID: <season-1@test.local>\r\n\
    Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
    --mix\r\n\
    Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
    The dates for the season.\r\n\
    --mix\r\n\
    Content-Type: text/calendar; name=\"season.ics\"\r\n\
    Content-Disposition: attachment; filename=\"season.ics\"\r\n\r\n";

/// Stitches `head`, `payload`, and the closing boundary into one raw source.
fn with_payload(head: &[u8], payload: &[u8], close: &[u8]) -> Vec<u8> {
    let mut source = head.to_vec();
    source.extend_from_slice(payload);
    source.extend_from_slice(close);
    source
}

#[tokio::test]
async fn the_invitations_own_ics_part_does_not_list_as_an_attachment() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let mut source = with_payload(GMAIL_INVITATION, REQUEST_PAYLOAD, b"--alt--\r\n");
    source.extend_from_slice(GMAIL_ATTACHMENT);
    source.extend_from_slice(REQUEST_PAYLOAD);
    source.extend_from_slice(b"--mix--\r\n");
    let fake = holding_source(CalendarFake::with_calendar_and_mail(), "gmail-1", &source);
    let app = opened_invitation(fake, &surfaces, "gmail-1").await;

    let snapshot = app.reading().expect("the open invitation");
    assert_eq!(
        snapshot.invitation.as_ref().map(|block| block.kind),
        Some(InvitationKind::Rsvp),
        "the card stands in for the invitation"
    );
    assert!(
        snapshot.attachments.is_empty(),
        "the part the card consumed lists nowhere, the body part nor the attached \
         duplicate: {:?}",
        snapshot.attachments,
    );
}

#[tokio::test]
async fn an_ordinary_attached_calendar_file_keeps_its_row() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let source = with_payload(ATTACHED_CALENDAR, PUBLISH_PAYLOAD, b"--mix--\r\n");
    let fake = holding_source(CalendarFake::with_calendar_and_mail(), "season-1", &source);
    let app = opened_invitation(fake, &surfaces, "season-1").await;

    let snapshot = app.reading().expect("the open message");
    assert_eq!(
        snapshot.invitation, None,
        "a published calendar file is no invitation"
    );
    assert_eq!(snapshot.attachments.len(), 1);
    assert_eq!(snapshot.attachments[0].file_name, "season.ics");
    assert_eq!(snapshot.attachments[0].media_type, "text/calendar");
}
