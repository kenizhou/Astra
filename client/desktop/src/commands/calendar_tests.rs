// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar commands: the snapshot pull round trips the
//! core's published view whole, the brief generate threads the account
//! and the civil day through to the core and answers its view, and a
//! generation's typed failure maps onto the command error the webview
//! sees with no content riding it.

use std::sync::Arc;

use astra_app::{BriefDocumentError, BriefMeetingRow, MeetingBriefState, MeetingBriefView};
use astra_viewmodel::{CalendarRow, CalendarSnapshot, OccurrenceRow};
use engine_api::{CalendarDate, CalendarId, ProviderKey, UtcDateTime};

use super::{calendar_brief_generate_of, calendar_of};
use crate::commands::CommandError;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The fixture account the calendar commands name.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").expect("an account id")
}

/// The civil day the brief tests ask for.
fn day() -> CalendarDate {
    CalendarDate::new(2026, 10, 19).expect("a civil day")
}

/// A UTC instant, the way the occurrence literal names one.
fn utc(value: &str) -> UtcDateTime {
    value.parse().unwrap()
}

/// One seeded brief answer: a generated day holding one meeting.
fn generated_brief() -> MeetingBriefView {
    MeetingBriefView {
        account: account(),
        day: day(),
        state: MeetingBriefState::Generated,
        brief: "The standup opens the day.".to_owned(),
        meetings: vec![BriefMeetingRow {
            title: "Standup".to_owned(),
            start: utc("2026-10-19T07:00:00Z"),
            end: utc("2026-10-19T08:00:00Z"),
            all_day: false,
            organizer: None,
            attendees: Vec::new(),
            location: None,
            invitation_subject: None,
        }],
    }
}

#[test]
fn the_calendar_pull_round_trips_the_published_snapshot() {
    let app = FakeApp::new();
    let snapshot = CalendarSnapshot {
        calendars: vec![CalendarRow {
            account: account(),
            id: CalendarId::try_from("cal-1").unwrap(),
            name: "Work".to_owned(),
            color: Some("#039be5".to_owned()),
            visible: true,
            can_write: true,
            is_default: true,
        }],
        occurrences: vec![OccurrenceRow {
            account: account(),
            event: ProviderKey::new("/dav/cal/work/default/evt-1.ics").unwrap(),
            calendar: CalendarId::try_from("cal-1").unwrap(),
            title: "Standup".to_owned(),
            start: utc("2026-10-19T07:00:00Z"),
            end: utc("2026-10-19T07:30:00Z"),
            all_day: false,
            recurring: true,
            overridden: false,
            occurrence_start: None,
            accuracy: astra_viewmodel::OccurrenceAccuracy::Zoned,
            participation: astra_viewmodel::Participation::Accepted,
            can_move: true,
            series_edit_warns: false,
        }],
        write_states: Vec::new(),
        reply_prompts: Vec::new(),
        now_data: None,
    };
    *app.calendar.lock().unwrap() = snapshot.clone();
    let state = state("calendar-pull", Arc::clone(&app), FakeStore::accepting());

    assert_eq!(calendar_of(&state), snapshot, "the pull answers whole");
}

#[test]
fn the_brief_generate_threads_the_account_and_day_and_answers_its_view() {
    let app = FakeApp::new();
    app.brief.lock().unwrap().brief = Ok(generated_brief());
    let state = state("calendar-brief", Arc::clone(&app), FakeStore::accepting());
    let account = account();

    let view = state
        .runtime
        .block_on(|| calendar_brief_generate_of(&state, &account, day()))
        .expect("the brief answers");
    assert_eq!(view, generated_brief());
    assert_eq!(
        app.brief.lock().unwrap().brief_into.as_slice(),
        [("work".to_owned(), "2026-10-19".to_owned())].as_slice(),
        "the account and the civil day crossed to the core, the day as its wire word",
    );
}

#[test]
fn a_brief_refusal_surfaces_the_sentence_and_never_panics() {
    let app = FakeApp::new();
    app.brief.lock().unwrap().brief = Err(BriefDocumentError::NoBundle);
    let state = state(
        "calendar-brief-refused",
        Arc::clone(&app),
        FakeStore::accepting(),
    );
    let account = account();

    let refused = state
        .runtime
        .block_on(|| calendar_brief_generate_of(&state, &account, day()))
        .expect_err("the refusal crosses as the command error");
    assert_eq!(
        refused.to_string(),
        CommandError::Ai("no ai bundle is running".to_owned()).to_string(),
        "the typed failure maps onto the sentence the webview sees",
    );
}

#[test]
fn the_no_meetings_view_crosses_as_its_wire_literal() {
    let view = MeetingBriefView {
        account: account(),
        day: day(),
        state: MeetingBriefState::NoMeetings,
        brief: String::new(),
        meetings: Vec::new(),
    };
    let wire = serde_json::to_value(&view).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": "work",
            "day": "2026-10-19",
            "state": "no_meetings"
        }),
    );
}
