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
    assert_eq!(wire, raw, "the fixture round trips byte for byte");
}
