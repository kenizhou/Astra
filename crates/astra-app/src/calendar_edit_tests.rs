// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the CRUD mapping: the create draft, the form-preserving patch,
//! the one-vs-all split, the series shift, and the drag maths, all against
//! hand-built stored events (the engine's patcher, and the dispatch tests over
//! the fakes, pin the wire side). The repeat-rule editability round trip
//! lives in its own file beside this one.

use engine_api::{
    CalendarDate, CalendarDateTime, Frequency, RecurrenceBound, TimeZoneId, Uid, Weekday,
};

use super::{build_deletion, build_draft, build_patch, CalendarEditError, NewEvent};
use crate::calendar_fakes::{all_day_event, local, utc, weekly_event, zoned_event};
use crate::protocol::{EventEdit, RecurrenceChange, RepeatEnd, RepeatRule};

/// A fixed stamp, so the patch assertions are deterministic.
fn stamp() -> engine_api::UtcDateTime {
    utc("2026-10-19T10:00:00Z")
}

/// A minted uid, the way the dispatch mints one.
fn uid() -> Uid {
    Uid::new("astra-1@test.local").unwrap()
}

/// A zoned wall clock in Amsterdam, the way the edit literals state one.
fn zoned(value: &str) -> CalendarDateTime {
    CalendarDateTime::Zoned {
        local: local(value),
        zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
    }
}

/// A one-off create over caller-held clocks, the shape every create test
/// starts from.
fn one_off<'a>(start: &'a CalendarDateTime, end: &'a CalendarDateTime) -> NewEvent<'a> {
    NewEvent {
        calendar: engine_api::CalendarId::try_from("cal-1").unwrap(),
        summary: "Standup",
        start,
        end,
        description: None,
        location: None,
        recurrence: None,
    }
}

/// The standup's clocks, bound once so the create fixtures can borrow them.
fn standup_clocks() -> (CalendarDateTime, CalendarDateTime) {
    (zoned("2026-10-19T09:00:00"), zoned("2026-10-19T09:30:00"))
}

#[test]
fn a_create_maps_onto_the_engine_draft_with_the_minted_uid_and_stamp() {
    let (start, end) = standup_clocks();
    let draft = build_draft(&one_off(&start, &end), uid(), stamp(), Weekday::Mo).unwrap();
    assert_eq!(draft.uid.as_str(), "astra-1@test.local");
    assert_eq!(draft.summary, "Standup");
    assert_eq!(draft.start, zoned("2026-10-19T09:00:00"));
    assert_eq!(draft.end, zoned("2026-10-19T09:30:00"));
    assert_eq!(draft.stamp, stamp());
    assert!(draft.recurrence.is_none());
    assert!(draft.description.is_none());
    assert!(draft.location.is_none());
}

#[test]
fn a_create_carries_its_notes_location_and_repeat_rule() {
    let rule = RepeatRule {
        frequency: Frequency::Weekly,
        interval: 1,
        weekdays: vec![Weekday::Tu, Weekday::Th],
        end: RepeatEnd::Never,
    };
    let (start, end) = standup_clocks();
    let new = NewEvent {
        description: Some("Notes"),
        location: Some("Room 1"),
        recurrence: Some(&rule),
        ..one_off(&start, &end)
    };
    let draft = build_draft(&new, uid(), stamp(), Weekday::Mo).unwrap();
    assert_eq!(draft.description.as_deref(), Some("Notes"));
    assert_eq!(draft.location.as_deref(), Some("Room 1"));
    let repeat = draft.recurrence.as_ref().unwrap();
    assert_eq!(repeat.until, None, "a never-ending rule resolves no bound");
    assert_eq!(repeat.rule.frequency, Frequency::Weekly);
    assert_eq!(repeat.rule.by_day.len(), 2);
}

#[test]
fn an_on_date_rule_ends_at_the_last_second_of_the_named_day() {
    // The inclusive-date rule: a series ending on the 21st still has its
    // occurrence ON the 21st, timed or all-day alike, so the bound is the
    // day's final second rather than its midnight.
    let rule = RepeatRule {
        frequency: Frequency::Daily,
        interval: 1,
        weekdays: Vec::new(),
        end: RepeatEnd::OnDate(CalendarDate::new(2026, 10, 21).unwrap()),
    };
    let (start, end) = standup_clocks();
    let new = NewEvent {
        recurrence: Some(&rule),
        ..one_off(&start, &end)
    };
    let draft = build_draft(&new, uid(), stamp(), Weekday::Mo).unwrap();
    let repeat = draft.recurrence.as_ref().unwrap();
    assert_eq!(
        repeat.rule.bound,
        RecurrenceBound::Until(local("2026-10-21T23:59:59"))
    );
    // A zoned event's UNTIL reaches iCalendar as an instant resolved through
    // the event's own zone (RFC 5545 §3.3.10), and it travels with the rule.
    let bound = zoned("2026-10-21T23:59:59");
    let resolved = engine_api::resolve_instant(&bound).unwrap().unwrap();
    assert_eq!(repeat.until, Some(resolved));
}

#[test]
fn an_after_count_rule_maps_onto_a_counted_bound() {
    let rule = RepeatRule {
        frequency: Frequency::Daily,
        interval: 2,
        weekdays: Vec::new(),
        end: RepeatEnd::AfterCount(5),
    };
    let (start, end) = standup_clocks();
    let new = NewEvent {
        recurrence: Some(&rule),
        ..one_off(&start, &end)
    };
    let repeat = build_draft(&new, uid(), stamp(), Weekday::Mo)
        .unwrap()
        .recurrence
        .unwrap();
    assert_eq!(repeat.rule.interval.get(), 2);
    assert!(matches!(repeat.rule.bound, RecurrenceBound::Count(count) if count.get() == 5));
    assert_eq!(repeat.until, None, "a counted rule has no UNTIL to resolve");
}

#[test]
fn an_edit_patches_the_title_and_notes_in_their_three_states() {
    let event = zoned_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let edit = EventEdit {
        title: Some("Renamed".to_owned()),
        notes: Some(String::new()),
        location: Some("Room 2".to_owned()),
        ..EventEdit::default()
    };
    let (_, patch) = build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap();
    assert_eq!(patch.summary_edit(), Some("Renamed"));
    assert_eq!(patch.description_edit(), Some(&engine_api::TextEdit::Clear));
    assert_eq!(
        patch.location_edit(),
        Some(&engine_api::TextEdit::Set("Room 2".to_owned()))
    );
    assert!(patch.start_edit().is_none() && patch.end_edit().is_none());
}

#[test]
fn an_edit_renders_its_clocks_in_the_stored_event_own_form() {
    // A zoned event stays in its own zone and an all-day event stays a date:
    // the core cannot even express the conversion the engine's form guard
    // refuses.
    let event = zoned_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let edit = EventEdit {
        start: Some(local("2026-10-20T11:00:00")),
        end: Some(local("2026-10-20T11:30:00")),
        ..EventEdit::default()
    };
    let (_, patch) = build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap();
    assert_eq!(
        patch.start_edit(),
        Some(&CalendarDateTime::Zoned {
            local: local("2026-10-20T11:00:00"),
            zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
        })
    );

    let day = all_day_event("evt-2", "Away day", "2026-10-19");
    let edit = EventEdit {
        start: Some(local("2026-10-21T00:00:00")),
        end: Some(local("2026-10-22T00:00:00")),
        ..EventEdit::default()
    };
    let (_, patch) = build_patch(&day, &edit, stamp(), Weekday::Mo).unwrap();
    assert_eq!(
        patch.start_edit(),
        Some(&CalendarDateTime::Date(
            CalendarDate::new(2026, 10, 21).unwrap()
        ))
    );
}

#[test]
fn an_occurrence_edit_targets_the_instance_and_carries_its_resolved_instant() {
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let original = local("2026-10-26T09:00:00");
    let edit = EventEdit {
        start: Some(original),
        end: Some(local("2026-10-26T10:00:00")),
        occurrence: Some(original),
        ..EventEdit::default()
    };
    let (target, _) = build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap();
    match target {
        engine_api::PatchTarget::Instance(occurrence) => {
            assert_eq!(
                occurrence.start,
                CalendarDateTime::Zoned {
                    local: original,
                    zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
                }
            );
            assert!(occurrence.instant.is_some(), "the instant is resolved here");
        }
        other @ engine_api::PatchTarget::Series => {
            panic!("a series target for an occurrence edit: {other:?}");
        }
    }
}

#[test]
fn a_repeat_rule_belongs_to_the_series_not_to_one_occurrence() {
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let edit = EventEdit {
        recurrence: Some(RecurrenceChange::Clear),
        occurrence: Some(local("2026-10-26T09:00:00")),
        ..EventEdit::default()
    };
    assert_eq!(
        build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap_err(),
        CalendarEditError::RuleOnOccurrence
    );
}

#[test]
fn an_occurrence_content_edit_carries_both_clocks_unchanged() {
    // The engine's CalDAV path refuses a first-touch instance patch without
    // both clocks, because the copy would inherit the master's slot. A host
    // that edits only the content states no clocks, so both come back as the
    // occurrence's own unchanged times.
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let edit = EventEdit {
        title: Some("Renamed".to_owned()),
        occurrence: Some(local("2026-11-09T09:00:00")),
        ..EventEdit::default()
    };
    let (_, patch) = build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap();
    let zone = TimeZoneId::iana("Europe/Amsterdam").unwrap();
    assert_eq!(
        patch.start_edit(),
        Some(&CalendarDateTime::Zoned {
            local: local("2026-11-09T09:00:00"),
            zone: zone.clone(),
        })
    );
    assert_eq!(
        patch.end_edit(),
        Some(&CalendarDateTime::Zoned {
            local: local("2026-11-09T10:00:00"),
            zone,
        })
    );
}

#[test]
fn an_occurrence_edit_with_one_edge_only_is_refused() {
    // One edge without the other is neither a content edit nor a complete
    // move; guessing the missing half would silently re-time the occurrence.
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let edit = EventEdit {
        start: Some(local("2026-11-09T10:00:00")),
        occurrence: Some(local("2026-11-09T09:00:00")),
        ..EventEdit::default()
    };
    assert_eq!(
        build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap_err(),
        CalendarEditError::OccurrenceNeedsBothEdges
    );
}

#[test]
fn an_occurrence_fill_keeps_absolute_duration_across_a_dst_boundary() {
    // The engine materialises every occurrence's end as the master's
    // absolute duration after that occurrence's own start: an overnight
    // master whose own span crosses the spring-forward renders 22:00 to
    // 07:00 (nine wall hours, eight elapsed), while every later occurrence
    // renders 22:00 to 06:00 the next morning (eight wall hours). The fill
    // must state the same end, or a content-only edit silently re-times the
    // occurrence's end by the transition.
    let mut event = weekly_event("evt-1", "Nightly", "2026-03-28T22:00:00");
    event.duration = engine_api::Duration::from_parts(0, 0, 8, 0, 0, 0).unwrap();
    let edit = EventEdit {
        title: Some("Renamed".to_owned()),
        occurrence: Some(local("2026-04-04T22:00:00")),
        ..EventEdit::default()
    };
    let (_, patch) = build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap();
    assert_eq!(
        patch.end_edit(),
        Some(&CalendarDateTime::Zoned {
            local: local("2026-04-05T06:00:00"),
            zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
        })
    );
}

#[test]
fn clocks_read_from_one_occurrence_become_the_series_shift() {
    // An editor opened on the third occurrence shows its times; writing them
    // onto the master would delete every earlier occurrence. Naming where the
    // clocks came from turns the edit into the shift the user made.
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let read_from = local("2026-11-02T09:00:00");
    let edit = EventEdit {
        start: Some(local("2026-11-02T11:00:00")),
        end: Some(local("2026-11-02T11:30:00")),
        times_from_occurrence: Some(read_from),
        ..EventEdit::default()
    };
    let (target, patch) = build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap();
    assert!(matches!(target, engine_api::PatchTarget::Series));
    assert_eq!(
        patch.start_edit(),
        Some(&CalendarDateTime::Zoned {
            local: local("2026-10-19T11:00:00"),
            zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
        }),
        "the series' own start, shifted by the two hours the user moved it"
    );
    // A shift without both edges is not a shift.
    let half = EventEdit {
        start: Some(local("2026-11-02T11:00:00")),
        times_from_occurrence: Some(read_from),
        ..EventEdit::default()
    };
    assert_eq!(
        build_patch(&event, &half, stamp(), Weekday::Mo).unwrap_err(),
        CalendarEditError::ShiftNeedsBothEdges
    );
}

#[test]
fn a_delete_targets_the_whole_series_or_the_one_named_occurrence() {
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let all = build_deletion(&event, None, stamp()).unwrap();
    assert!(all.occurrence_target().is_none());
    assert_eq!(all.uid, event.uid);

    let original = local("2026-10-26T09:00:00");
    let one = build_deletion(&event, Some(original), stamp()).unwrap();
    let occurrence = one.occurrence_target().unwrap();
    assert_eq!(
        occurrence.start,
        CalendarDateTime::Zoned {
            local: original,
            zone: TimeZoneId::iana("Europe/Amsterdam").unwrap(),
        }
    );
    assert!(occurrence.instant.is_some());
}
