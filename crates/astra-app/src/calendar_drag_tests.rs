// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the drag maths: whole-block moves that preserve the duration,
//! edge resizes clamped at the quarter-hour floor (a whole day when all-day),
//! the all-day minute drop, the year bound, and the occurrence a recurring
//! drag names.

use super::{apply_drag, CalendarEditError};
use crate::calendar::edit::build_patch;
use crate::calendar_fakes::{all_day_event, local, utc, weekly_event, zoned_event};
use crate::protocol::{EventDrag, EventEdge};

/// A one-off timed event, the shape every drag test starts from.
fn standup() -> engine_api::Event {
    zoned_event("evt-1", "Standup", "2026-10-19T09:00:00")
}

#[test]
fn a_whole_drag_moves_both_edges_and_preserves_the_duration() {
    let event = standup();
    let drag = EventDrag {
        edge: EventEdge::Whole,
        days: 1,
        minutes: -30,
        occurrence: None,
    };
    let edit = apply_drag(&event, &drag).unwrap();
    assert_eq!(edit.start, Some(local("2026-10-20T08:30:00")));
    assert_eq!(edit.end, Some(local("2026-10-20T09:30:00")));
    assert_eq!(edit.occurrence, None);
}

#[test]
fn a_dragged_edge_clamps_at_a_quarter_hour_minimum() {
    let event = standup();
    // Dragging the start past the end clamps to the end less fifteen minutes.
    let drag = EventDrag {
        edge: EventEdge::Start,
        days: 0,
        minutes: 90,
        occurrence: None,
    };
    let edit = apply_drag(&event, &drag).unwrap();
    assert_eq!(edit.start, Some(local("2026-10-19T09:45:00")));
    assert_eq!(
        edit.end,
        Some(local("2026-10-19T10:00:00")),
        "the end did not move"
    );
    // Dragging the end before the start clamps to the start plus fifteen.
    let drag = EventDrag {
        edge: EventEdge::End,
        days: 0,
        minutes: -60,
        occurrence: None,
    };
    let edit = apply_drag(&event, &drag).unwrap();
    assert_eq!(edit.start, Some(local("2026-10-19T09:00:00")));
    assert_eq!(edit.end, Some(local("2026-10-19T09:15:00")));
}

#[test]
fn an_all_day_drag_drops_its_minutes_and_keeps_a_whole_day_minimum() {
    let day = all_day_event("evt-1", "Away day", "2026-10-19");
    // A minute component on an all-day event is a whole-day move, not a
    // rounding onto the neighbouring date.
    let drag = EventDrag {
        edge: EventEdge::Whole,
        days: 2,
        minutes: 90,
        occurrence: None,
    };
    let edit = apply_drag(&day, &drag).unwrap();
    assert_eq!(edit.start, Some(local("2026-10-21T00:00:00")));
    assert_eq!(edit.end, Some(local("2026-10-22T00:00:00")));
    // A resize cannot shrink below one whole day.
    let drag = EventDrag {
        edge: EventEdge::End,
        days: -1,
        minutes: 0,
        occurrence: None,
    };
    let edit = apply_drag(&day, &drag).unwrap();
    assert_eq!(edit.end, Some(local("2026-10-20T00:00:00")));
}

#[test]
fn a_drag_beyond_a_year_is_refused_rather_than_wrapping() {
    let event = standup();
    let drag = EventDrag {
        edge: EventEdge::Whole,
        days: 400,
        minutes: 0,
        occurrence: None,
    };
    assert!(matches!(
        apply_drag(&event, &drag).unwrap_err(),
        CalendarEditError::DragTooFar(_)
    ));
}

#[test]
fn a_drag_names_one_occurrence_by_its_original_start() {
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let original = local("2026-10-26T09:00:00");
    let drag = EventDrag {
        edge: EventEdge::Whole,
        days: 1,
        minutes: 0,
        occurrence: Some(original),
    };
    let edit = apply_drag(&event, &drag).unwrap();
    assert_eq!(edit.occurrence, Some(original));
    let (target, _) = build_patch(&event, &edit, utc("2026-10-19T10:00:00Z")).unwrap();
    assert!(matches!(target, engine_api::PatchTarget::Instance(_)));
}

#[test]
fn dragging_a_later_occurrence_shifts_its_own_clock_not_the_masters() {
    // The engine writes an override's times from this patch, and the master's
    // bounds are the FIRST occurrence's times: shifting those for a later
    // occurrence would land the block in the series' opening slot.
    let event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let drag = EventDrag {
        edge: EventEdge::Whole,
        days: 1,
        minutes: 30,
        occurrence: Some(local("2026-11-09T09:00:00")),
    };
    let edit = apply_drag(&event, &drag).unwrap();
    assert_eq!(edit.start, Some(local("2026-11-10T09:30:00")));
    assert_eq!(edit.end, Some(local("2026-11-10T10:30:00")));

    // An edge resize of that occurrence rides its own clock too: the dragged
    // end moves from the occurrence's end, clamped at its own start.
    let drag = EventDrag {
        edge: EventEdge::End,
        days: 0,
        minutes: -120,
        occurrence: Some(local("2026-11-09T09:00:00")),
    };
    let edit = apply_drag(&event, &drag).unwrap();
    assert_eq!(edit.start, Some(local("2026-11-09T09:00:00")));
    assert_eq!(edit.end, Some(local("2026-11-09T09:15:00")));
}
