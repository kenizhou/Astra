// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar view-model: the exact JSON wire shape a host pulls after a
//! `calendar` signal, that it reads back identical, and the default an app publishes
//! before any calendar state exists.

use engine_api::{AccountId, CalendarId, LocalDateTime, ProviderKey, UtcDateTime};

use crate::{
    CalendarRow, CalendarSnapshot, CalendarWriteRow, CalendarWriteState, OccurrenceAccuracy,
    OccurrenceRow, Participation, ReplyPromptRow,
};

/// A well-formed account id, the way every calendar test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A UTC instant, the way the occurrence literals name one.
fn utc(value: &str) -> UtcDateTime {
    value.parse().unwrap()
}

#[test]
fn the_wire_shape_is_snake_case_and_carries_every_row_kind() {
    let snapshot = CalendarSnapshot {
        calendars: vec![CalendarRow {
            account: account("work"),
            id: CalendarId::try_from("cal-1").unwrap(),
            name: "Work".to_owned(),
            color: Some("#039be5".to_owned()),
            visible: true,
            can_write: true,
            is_default: true,
        }],
        occurrences: vec![OccurrenceRow {
            account: account("work"),
            event: ProviderKey::new("/dav/cal/work/default/evt-1.ics").unwrap(),
            calendar: CalendarId::try_from("cal-1").unwrap(),
            title: "Standup".to_owned(),
            start: utc("2026-10-19T07:00:00Z"),
            end: utc("2026-10-19T07:30:00Z"),
            all_day: false,
            recurring: true,
            overridden: true,
            occurrence_start: Some(LocalDateTime::new(2026, 10, 19, 9, 0, 0).unwrap()),
            accuracy: OccurrenceAccuracy::Zoned,
            participation: Participation::Tentative,
            can_move: false,
            series_edit_warns: true,
        }],
        write_states: vec![CalendarWriteRow {
            account: account("work"),
            state: CalendarWriteState::Writing,
        }],
        reply_prompts: vec![ReplyPromptRow {
            account: account("work"),
            message: ProviderKey::new("imap:v1:u7@INBOX").unwrap(),
            summary: "Sprint planning".to_owned(),
            organizer: "boss@example.test".to_owned(),
            response: Participation::Accepted,
            comment: None,
            status_code: "5.2".to_owned(),
        }],
        now_data: Some(crate::NowLineData {
            now: utc("2026-10-19T08:12:00Z"),
            zone: "Europe/Amsterdam".to_owned(),
        }),
        shape: None,
    };

    let wire = serde_json::to_value(&snapshot).expect("the snapshot serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "calendars": [
                {
                    "account": "work",
                    "id": "cal-1",
                    "name": "Work",
                    "color": "#039be5",
                    "visible": true,
                    "can_write": true,
                    "is_default": true
                }
            ],
            "occurrences": [
                {
                    "account": "work",
                    "event": "/dav/cal/work/default/evt-1.ics",
                    "calendar": "cal-1",
                    "title": "Standup",
                    "start": "2026-10-19T07:00:00Z",
                    "end": "2026-10-19T07:30:00Z",
                    "all_day": false,
                    "recurring": true,
                    "overridden": true,
                    "occurrence_start": "2026-10-19T09:00:00",
                    "accuracy": "zoned",
                    "participation": "tentative",
                    "can_move": false,
                    "series_edit_warns": true
                }
            ],
            "write_states": [
                {
                    "account": "work",
                    "state": "writing"
                }
            ],
            "reply_prompts": [
                {
                    "account": "work",
                    "message": "imap:v1:u7@INBOX",
                    "summary": "Sprint planning",
                    "organizer": "boss@example.test",
                    "response": "accepted",
                    "status_code": "5.2"
                }
            ],
            "now_data": {
                "now": "2026-10-19T08:12:00Z",
                "zone": "Europe/Amsterdam"
            }
        }),
    );

    let back: CalendarSnapshot = serde_json::from_value(wire).expect("the snapshot reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn the_accuracy_words_and_write_states_cross_as_snake_case() {
    // Each accuracy and write-state word is pinned both ways: a host branches on these,
    // so a renamed word must never slip out silently.
    for (accuracy, word) in [
        (OccurrenceAccuracy::Zoned, r#""zoned""#),
        (OccurrenceAccuracy::Floating, r#""floating""#),
        (OccurrenceAccuracy::AllDay, r#""all_day""#),
    ] {
        assert_eq!(serde_json::to_string(&accuracy).unwrap(), word);
        assert_eq!(
            serde_json::from_str::<OccurrenceAccuracy>(word).unwrap(),
            accuracy
        );
    }
    for (state, word) in [
        (CalendarWriteState::Writing, r#""writing""#),
        (
            CalendarWriteState::FailedWithRetry,
            r#""failed_with_retry""#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&state).unwrap(), word);
        assert_eq!(
            serde_json::from_str::<CalendarWriteState>(word).unwrap(),
            state
        );
    }
}

#[test]
fn the_participation_words_cross_as_snake_case_and_default_to_a_commitment() {
    // Each participation word is pinned both ways, and the additive-wire rule
    // with it: a row from before the field existed parses as a commitment
    // (never an unanswered hold), so an older snapshot draws every block it
    // ever drew.
    for (participation, word) in [
        (Participation::NeedsAction, r#""needs_action""#),
        (Participation::Accepted, r#""accepted""#),
        (Participation::Declined, r#""declined""#),
        (Participation::Tentative, r#""tentative""#),
        (Participation::Delegated, r#""delegated""#),
    ] {
        assert_eq!(serde_json::to_string(&participation).unwrap(), word);
        assert_eq!(
            serde_json::from_str::<Participation>(word).unwrap(),
            participation
        );
    }
    assert_eq!(Participation::default(), Participation::Accepted);
    let old_wire = serde_json::json!({
        "account": "work",
        "event": "evt-1",
        "calendar": "cal-1",
        "title": "Standup",
        "start": "2026-10-19T07:00:00Z",
        "end": "2026-10-19T07:30:00Z",
        "all_day": false,
        "recurring": false,
        "overridden": false,
        "accuracy": "zoned"
    });
    let row: OccurrenceRow = serde_json::from_value(old_wire).expect("the old wire parses");
    assert_eq!(row.participation, Participation::Accepted);
    assert!(!row.can_move);
    assert!(
        !row.series_edit_warns,
        "a row from before the warning existed never warns"
    );
}

#[test]
fn the_default_snapshot_crosses_as_an_empty_object() {
    // Before any calendar state exists, the snapshot is empty and carries no now line:
    // nothing skips on the wire except by being absent, so a cold host renders an empty
    // calendar, not a zeroed one.
    let snapshot = CalendarSnapshot::default();
    assert!(snapshot.calendars.is_empty());
    assert!(snapshot.occurrences.is_empty());
    assert!(snapshot.write_states.is_empty());
    assert!(snapshot.reply_prompts.is_empty());
    assert!(snapshot.now_data.is_none());

    let wire = serde_json::to_value(&snapshot).expect("the default serialises");
    assert_eq!(wire, serde_json::json!({}));

    let back: CalendarSnapshot = serde_json::from_value(serde_json::to_value(&snapshot).unwrap())
        .expect("the default reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn a_row_without_a_colour_or_an_occurrence_token_omits_them() {
    // A calendar with no colour crosses no colour key; an occurrence of a one-off event
    // crosses no occurrence token and no override marker confusion: `overridden` is a
    // plain flag and always crosses.
    let snapshot = CalendarSnapshot {
        calendars: vec![CalendarRow {
            account: account("home"),
            id: CalendarId::try_from("personal").unwrap(),
            name: "Personal".to_owned(),
            color: None,
            visible: false,
            can_write: false,
            is_default: false,
        }],
        occurrences: vec![OccurrenceRow {
            account: account("home"),
            event: ProviderKey::new("/dav/cal/home/personal/dentist.ics").unwrap(),
            calendar: CalendarId::try_from("personal").unwrap(),
            title: "Dentist".to_owned(),
            start: utc("2026-10-21T08:00:00Z"),
            end: utc("2026-10-21T09:00:00Z"),
            all_day: false,
            recurring: false,
            overridden: false,
            occurrence_start: None,
            accuracy: OccurrenceAccuracy::Floating,
            participation: Participation::default(),
            can_move: false,
            series_edit_warns: false,
        }],
        write_states: Vec::new(),
        reply_prompts: Vec::new(),
        now_data: None,
        shape: None,
    };

    let wire = serde_json::to_value(&snapshot).expect("the snapshot serialises");
    assert!(
        wire["calendars"][0].get("color").is_none(),
        "an absent colour crosses no key: {wire}"
    );
    assert!(
        wire["occurrences"][0].get("occurrence_start").is_none(),
        "a one-off event crosses no occurrence token: {wire}"
    );
    assert!(
        wire.get("write_states").is_none(),
        "no write states cross no key: {wire}"
    );
    assert!(
        wire.get("now_data").is_none(),
        "no now line crosses no key: {wire}"
    );
    assert!(
        wire.get("shape").is_none(),
        "no standing shape crosses no key: {wire}"
    );

    let back: CalendarSnapshot = serde_json::from_value(wire).expect("reads back");
    assert_eq!(back, snapshot);
}

#[test]
fn the_shared_web_fixture_parses_as_the_snapshot() {
    // The web service's calendar.test.ts parses this same file, so the
    // fixture the surface mirrors and the snapshot the host publishes can
    // never drift apart.
    let raw = include_str!("../../../client/web/src/services/__fixtures__/calendar_snapshot.json");
    let back: CalendarSnapshot = serde_json::from_str(raw).expect("the fixture parses");
    let wire = serde_json::to_value(&back).expect("the snapshot serialises");
    let raw: serde_json::Value = serde_json::from_str(raw).expect("the fixture is JSON");
    assert_eq!(wire, raw, "the fixture round trips value for value");
}

#[test]
fn the_detail_wire_crosses_the_roll_call_and_the_rule() {
    // The detail read's wire: the roll-call's rows skip their absent words
    // (a plain attendee carries no role key), the folded tail crosses only
    // when a cap folded one, and the rule carries the engine's own frequency
    // and weekday words beside the app's sentence.
    let detail = crate::EventDetail {
        account: account("work"),
        event: ProviderKey::new("/dav/cal/work/default/evt-1.ics").unwrap(),
        participants: vec![
            crate::ParticipantRow {
                name_or_address: "Boss".to_owned(),
                role: Some("organiser".to_owned()),
                response: Participation::Accepted,
            },
            crate::ParticipantRow {
                name_or_address: "kim@example.test".to_owned(),
                role: None,
                response: Participation::NeedsAction,
            },
        ],
        folded_participants: 3,
        rule: Some(crate::RuleRead {
            frequency: engine_api::Frequency::Weekly,
            interval: 3,
            days: vec![engine_api::Weekday::Mo, engine_api::Weekday::We],
            end: Some(crate::RuleEndRead::AfterCount(10)),
            sentence: "Repeats every third week on Mondays and Wednesdays, ten times.".to_owned(),
        }),
    };

    let wire = serde_json::to_value(&detail).expect("the detail serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": "work",
            "event": "/dav/cal/work/default/evt-1.ics",
            "participants": [
                {
                    "name_or_address": "Boss",
                    "role": "organiser",
                    "response": "accepted"
                },
                {
                    "name_or_address": "kim@example.test",
                    "response": "needs_action"
                }
            ],
            "folded_participants": 3,
            "rule": {
                "frequency": "weekly",
                "interval": 3,
                "days": ["mo", "we"],
                "end": { "after_count": 10 },
                "sentence": "Repeats every third week on Mondays and Wednesdays, ten times."
            }
        })
    );
    let back: crate::EventDetail = serde_json::from_value(wire).expect("the detail reads back");
    assert_eq!(back, detail);
}

#[test]
fn an_empty_detail_and_a_fully_worded_shape_cross_only_what_stands() {
    // A one-off appointment with nobody on it crosses its keys alone, and
    // the shape crosses the anchor only where one stands: today's shape is
    // the view word by itself.
    let bare = crate::EventDetail {
        account: account("work"),
        event: ProviderKey::new("evt-2").unwrap(),
        participants: Vec::new(),
        folded_participants: 0,
        rule: None,
    };
    let wire = serde_json::to_value(&bare).expect("the bare detail serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": "work",
            "event": "evt-2"
        })
    );

    let view_only = crate::CalendarShape {
        view: crate::CalendarViewWire::Agenda,
        anchor: None,
    };
    assert_eq!(
        serde_json::to_value(&view_only).unwrap(),
        serde_json::json!({ "view": "agenda" })
    );
    let anchored = crate::CalendarShape {
        view: crate::CalendarViewWire::Month,
        anchor: Some(engine_api::CalendarDate::new(2026, 10, 6).unwrap()),
    };
    assert_eq!(
        serde_json::to_value(&anchored).unwrap(),
        serde_json::json!({ "view": "month", "anchor": "2026-10-06" })
    );
}
