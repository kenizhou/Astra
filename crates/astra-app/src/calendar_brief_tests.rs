// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the meeting brief: the one generation over a civil day's
//! occurrences and their linked invitation context (the MeetingBrief use
//! case under the turn's correlation), the no-meetings state that spends
//! no call, the prompt's redaction and untrusted wraps, the registry's
//! tier, the typed failure paths, and the view's wire shape.

use std::sync::{Arc, Mutex};

use astra_ai::{AiStore, CorrelationId, DlpPolicy, LlmUseCase, ModelTier};
use engine_api::{
    CalendarDate, CalendarDateTime, CalendarId, Event, EventId, Location, Participant,
    ParticipantRole, TimeZoneId, Uid,
};
use engine_core::membership::Memberships;

use super::{
    generate_meeting_brief, meeting_brief_prompt, BriefDocumentError, BriefMeeting,
    MeetingBriefError, MeetingBriefState, MEETING_BRIEF_PROMPT_VERSION,
};
use crate::calendar_fakes::{account, calendar_app, local, utc, zoned_event, CalendarFake};
use crate::dispatch_fakes::ai::{FixtureComplete, FixtureJudge};
use crate::{AiPass, CalendarIntent, Intent, Surface};

/// The account whose day the tests brief.
fn account_id() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").unwrap()
}

/// The civil day the diary fixtures live in.
fn day() -> CalendarDate {
    CalendarDate::new(2026, 10, 19).unwrap()
}

/// The good fixture: the one-field brief shape the output policy pins.
const GOOD: &str =
    r#"{"brief":"The standup opens the day; the planning needs the room booking confirmed."}"#;
const GOOD_TEXT: &str = "The standup opens the day; the planning needs the room booking confirmed.";

/// The stored copy of the invitation the fake's mailbox holds (the uid the
/// invitation source carries), moved onto the brief day with a room.
fn planning() -> Event {
    let mut organizer = Participant::attendee("boss@test.local");
    organizer.roles.insert(ParticipantRole::Owner);
    organizer.name = Some("Boss".to_owned());
    let mut attendee = Participant::attendee("info@test.local");
    attendee.name = Some("Info".to_owned());
    let mut event = Event::new(
        EventId::try_from("evt-9").unwrap(),
        Uid::new("meeting-1@test.local").unwrap(),
        Memberships::of_one(CalendarId::try_from("cal-1").unwrap()),
        CalendarDateTime::Zoned {
            local: local("2026-10-19T14:00:00"),
            zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
        },
    );
    event.title = "Sprint planning".to_owned();
    event.duration = engine_api::Duration::from_parts(0, 0, 1, 0, 0, 0).unwrap();
    event.participants = vec![organizer, attendee];
    event.locations = vec![Location {
        name: Some("Room A".to_owned()),
        description: None,
        location_types: std::collections::BTreeSet::new(),
        relative_to: None,
        coordinates: None,
        time_zone: None,
    }];
    event
}

/// The planning meeting as the pure service sees it: the drive's row for
/// the stored fixture, hand-built so the prompt tests need no engine.
fn planning_input() -> BriefMeeting {
    BriefMeeting {
        title: "Sprint planning".to_owned(),
        start: utc("2026-10-19T12:00:00Z"),
        end: utc("2026-10-19T13:00:00Z"),
        all_day: false,
        organizer: Some("Boss <boss@test.local>".to_owned()),
        attendees: vec!["Info <info@test.local>".to_owned()],
        location: Some("Room A".to_owned()),
        invitation_subject: Some("Sprint planning".to_owned()),
    }
}

/// Boots the app over `fake` with the scripted completion installed as the
/// AI pass, lands the mail (so the invitation lives in the store), and
/// syncs the diary over the fixture fortnight.
async fn briefed(
    fake: CalendarFake,
    complete: FixtureComplete,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> crate::App<CalendarFake> {
    let app = calendar_app(vec![account("work", fake)], surfaces).with_ai_pass(Some(AiPass::new(
        Box::new(FixtureJudge::scripted(&[])),
        Box::new(complete),
        Arc::new(AiStore::open_in_memory().unwrap()),
        DlpPolicy::Allow,
    )));
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app
}

/// The two-event day every drive test briefs: the plain standup and the
/// linked planning meeting.
fn two_event_day() -> CalendarFake {
    CalendarFake::with_calendar_and_mail()
        .holding(zoned_event("evt-1", "Standup", "2026-10-19T09:00:00"))
        .holding(planning())
}

#[tokio::test]
async fn a_two_event_day_briefs_both_meetings_with_the_occurrence_facts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&[GOOD]);
    let app = briefed(two_event_day(), complete.clone(), &surfaces).await;

    let view = app
        .generate_meeting_brief(&account_id(), day())
        .await
        .expect("the brief generates");
    assert_eq!(view.account, account_id());
    assert_eq!(view.day, day());
    assert_eq!(view.state, MeetingBriefState::Generated);
    assert_eq!(view.brief, GOOD_TEXT);
    assert_eq!(complete.call_count(), 1, "the brief is one call");

    assert_eq!(view.meetings.len(), 2, "both meetings row");
    let standup = &view.meetings[0];
    assert_eq!(standup.title, "Standup");
    assert_eq!(standup.start, utc("2026-10-19T07:00:00Z"));
    assert_eq!(standup.end, utc("2026-10-19T08:00:00Z"));
    assert!(!standup.all_day);
    assert!(
        standup.organizer.is_none() && standup.attendees.is_empty(),
        "a private appointment carries no invitation context"
    );
    assert!(
        standup.invitation_subject.is_none(),
        "no stored invitation names the standup"
    );

    let planning = &view.meetings[1];
    assert_eq!(planning.title, "Sprint planning");
    assert_eq!(planning.start, utc("2026-10-19T12:00:00Z"));
    assert_eq!(
        planning.organizer.as_deref(),
        Some("Boss <boss@test.local>")
    );
    assert_eq!(planning.attendees, ["Info <info@test.local>"]);
    assert_eq!(planning.location.as_deref(), Some("Room A"));
    assert_eq!(
        planning.invitation_subject.as_deref(),
        Some("Sprint planning"),
        "the stored invitation's message links by its uid"
    );

    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::MeetingBrief);
    assert_eq!(LlmUseCase::MeetingBrief.tier(), ModelTier::Economy);
    assert_eq!(MEETING_BRIEF_PROMPT_VERSION, "1");
    assert!(
        call.prompt.contains("Standup") && call.prompt.contains("Sprint planning"),
        "the prompt names both meetings:\n{}",
        call.prompt,
    );
}

#[tokio::test]
async fn the_brief_reads_the_store_not_the_drawn_snapshot() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&[GOOD]);
    let app = briefed(two_event_day(), complete.clone(), &surfaces).await;
    // Every calendar hidden: the drawn snapshot shows nothing, and the brief
    // still names the day's meetings, because it re-reads the account's
    // occurrences from the store rather than any snapshot field.
    app.dispatch(Intent::Calendar(CalendarIntent::SetVisibleCalendars {
        ids: vec![],
    }))
    .await
    .unwrap();
    assert!(
        app.calendar().occurrences.is_empty(),
        "the drawn snapshot holds nothing to read"
    );

    let view = app
        .generate_meeting_brief(&account_id(), day())
        .await
        .expect("the brief generates");
    assert_eq!(view.meetings.len(), 2, "the store's day, not the drawn set");
}

#[tokio::test]
async fn an_empty_day_answers_the_no_meetings_state_and_spends_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&[]);
    let app = briefed(two_event_day(), complete.clone(), &surfaces).await;

    let view = app
        .generate_meeting_brief(&account_id(), CalendarDate::new(2026, 10, 26).unwrap())
        .await
        .expect("the empty day answers, it does not fail");
    assert_eq!(view.state, MeetingBriefState::NoMeetings);
    assert_eq!(view.brief, "");
    assert!(view.meetings.is_empty());
    assert_eq!(complete.call_count(), 0, "no meetings, no call");
    let wire = serde_json::to_value(&view).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": "work",
            "day": "2026-10-26",
            "state": "no_meetings"
        }),
    );
}

#[test]
fn the_prompt_masks_addresses_and_keeps_the_timings_trusted() {
    let prompt = meeting_brief_prompt(day(), &[planning_input()]);
    assert!(
        !prompt.contains("boss@test.local") && !prompt.contains("info@test.local"),
        "no address crosses to the model:\n{prompt}",
    );
    assert!(
        prompt.matches('*').count() >= 30,
        "the attendee and organiser addresses mask, one character each:\n{prompt}",
    );
    assert!(
        prompt.contains("2026-10-19T12:00:00Z"),
        "the occurrence's timing rides the trusted half, unmasked:\n{prompt}",
    );
    assert_eq!(
        prompt.matches("<untrusted heading=\"meeting\">").count(),
        1,
        "each meeting rides its own untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("Title: Sprint planning")
            && prompt.contains("Location: Room A")
            && prompt.contains("Invitation subject: Sprint planning"),
        "the wrap carries the meeting's own facts:\n{prompt}",
    );
}

#[test]
fn the_generated_view_crosses_as_its_wire_literal() {
    let view = super::MeetingBriefView {
        account: account_id(),
        day: day(),
        state: MeetingBriefState::Generated,
        brief: GOOD_TEXT.to_owned(),
        meetings: vec![super::BriefMeetingRow {
            title: "Sprint planning".to_owned(),
            start: utc("2026-10-19T12:00:00Z"),
            end: utc("2026-10-19T13:00:00Z"),
            all_day: false,
            organizer: Some("Boss <boss@test.local>".to_owned()),
            attendees: vec!["Info <info@test.local>".to_owned()],
            location: Some("Room A".to_owned()),
            invitation_subject: Some("Sprint planning".to_owned()),
        }],
    };
    let wire = serde_json::to_value(&view).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": "work",
            "day": "2026-10-19",
            "state": "generated",
            "brief": GOOD_TEXT,
            "meetings": [
                {
                    "title": "Sprint planning",
                    "start": "2026-10-19T12:00:00Z",
                    "end": "2026-10-19T13:00:00Z",
                    "all_day": false,
                    "organizer": "Boss <boss@test.local>",
                    "attendees": ["Info <info@test.local>"],
                    "location": "Room A",
                    "invitation_subject": "Sprint planning"
                }
            ]
        }),
    );
    let back: super::MeetingBriefView = serde_json::from_value(wire).expect("the view reads back");
    assert_eq!(back, view);

    let bare = super::BriefMeetingRow {
        title: "Standup".to_owned(),
        start: utc("2026-10-19T07:00:00Z"),
        end: utc("2026-10-19T08:00:00Z"),
        all_day: false,
        organizer: None,
        attendees: Vec::new(),
        location: None,
        invitation_subject: None,
    };
    let wire = serde_json::to_value(&bare).expect("the bare row serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "title": "Standup",
            "start": "2026-10-19T07:00:00Z",
            "end": "2026-10-19T08:00:00Z",
            "all_day": false
        }),
        "an absent organiser, attendee set, location, and invitation cross no key",
    );
}

#[tokio::test]
async fn an_unparseable_reply_answers_the_typed_failure_never_a_panic() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let app = briefed(two_event_day(), complete, &surfaces).await;
    let failure = app
        .generate_meeting_brief(&account_id(), day())
        .await
        .expect_err("the unparseable output is a typed failure");
    assert!(
        matches!(
            &failure,
            BriefDocumentError::Brief(MeetingBriefError::MalformedOutput(_))
        ),
        "the failure is the malformed-output error: {failure}",
    );
}

#[tokio::test]
async fn a_failed_call_answers_the_typed_completion_error() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let complete = FixtureComplete::failing("llm unreachable: every attempt failed");
    let app = briefed(two_event_day(), complete, &surfaces).await;
    let failure = app
        .generate_meeting_brief(&account_id(), day())
        .await
        .expect_err("the seam failure is a typed failure");
    assert!(
        matches!(
            &failure,
            BriefDocumentError::Brief(MeetingBriefError::Completion(detail))
                if detail == "llm unreachable: every attempt failed"
        ),
        "the failure is the completion error: {failure}",
    );
}

#[tokio::test]
async fn the_correlation_lands_on_the_recorded_call() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let correlation = CorrelationId::generate();
    let brief = generate_meeting_brief(
        &account_id(),
        day(),
        &[planning_input()],
        &correlation,
        &complete,
    )
    .await
    .unwrap();
    assert_eq!(brief, GOOD_TEXT);
    let calls = complete.calls();
    assert_eq!(calls.len(), 1, "the fake ledger holds the one row");
    assert_eq!(
        calls[0].correlation, correlation,
        "the turn's correlation lands on the ledger row"
    );
}

#[tokio::test]
async fn an_app_without_the_ai_bundle_refuses_the_brief() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // The same synced diary over an app with no AI pass: the refusal is the
    // typed no-bundle failure, never a panic.
    let bare = calendar_app(vec![account("work", two_event_day())], &surfaces);
    let refusal = bare
        .generate_meeting_brief(&account_id(), day())
        .await
        .expect_err("no bundle, no generation");
    assert!(matches!(refusal, BriefDocumentError::NoBundle));
}
