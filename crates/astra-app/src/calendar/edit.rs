// SPDX-License-Identifier: GPL-3.0-only
//! The CRUD mapping: the calendar write intents rendered as the engine's own
//! create, patch, and delete shapes. Ported from mail-calendar's builders,
//! reshaped over Astra's engine-typed intents (no host strings to parse: the
//! intent already carries the engine's values).
//!
//! Three rules the engine forces and this module encodes, all mail-calendar's:
//!
//! - **A time an edit states is the event's own wall clock, rendered in the
//!   event's own form.** The engine's patcher rejects a form change outright (a
//!   zoned event moved to a UTC instant, an all-day event given a clock), so
//!   [`in_event_form`] renders every stated clock in whatever form the stored
//!   event already has: the core cannot even express the conversion the guard
//!   exists to refuse.
//! - **A recurrence the editor cannot describe in full is never written
//!   back.** A stored rule richer than [`RepeatRule`] (an nth-of-period
//!   weekday, a by-month-day pattern, two rules at once) read through a partial
//!   editor and saved would silently drop the parts the editor never saw, so a
//!   `Set` on one is refused rather than approximated.
//! - **A series edit seeded from one occurrence is a shift, not a move.**
//!   Clocks an editor read off the third occurrence are that occurrence's; the
//!   series' own start is the occurrence's clock less the span between them,
//!   which [`series_bounds_after`](super::drag::series_bounds_after) derives.

use std::num::NonZeroU32;

use engine_api::{
    resolve_instant, resolve_instant_in, CalendarDate, CalendarDateTime, DraftRecurrence, Event,
    EventDeletion, EventDraft, EventPatch, Frequency, LocalDateTime, Occurrence, PatchTarget,
    RecurrenceBound, RecurrenceRule, TimeZoneId, Uid, UtcDateTime,
};
use thiserror::Error;

use crate::protocol::{EventEdit, RecurrenceChange, RepeatEnd, RepeatRule};

use super::drag::{occurrence_bounds, series_bounds_after};

/// Why a write intent cannot be mapped onto an engine shape. Each names a rule
/// the caller (a surface, or a test) can act on, never a message to parse.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum CalendarEditError {
    /// The stored rule is richer than the editor can describe, so a
    /// replacement written from the editor's partial picture would silently
    /// drop the parts it never saw.
    #[error("this event's repeat rule is richer than the editor can describe")]
    RuleTooRich,
    /// A repeat rule belongs to the series; one occurrence is an instance of a
    /// rule, not a holder of one.
    #[error("a repeat rule belongs to the series, not to one occurrence")]
    RuleOnOccurrence,
    /// An edit whose clocks were read from one occurrence needs both of them to
    /// state the shift the user made.
    #[error("an edit shifted from an occurrence needs both its start and its end")]
    ShiftNeedsBothEdges,
    /// An occurrence edit that states one clock without the other: neither a
    /// content edit (which states neither) nor a complete move (which states
    /// both), and guessing the missing half would re-time the occurrence.
    #[error("an occurrence edit needs both its start and its end, or neither")]
    OccurrenceNeedsBothEdges,
    /// The drag exceeds the bound that keeps the civil-date arithmetic inside
    /// representable calendar time.
    #[error("a drag of {0} days is beyond the 366-day limit")]
    DragTooFar(i64),
    /// The occurrence token names no occurrence the store holds: stale from a
    /// series that changed under the view that minted it, or minted by
    /// nobody. The write is refused rather than split at a time the rule
    /// never produced.
    #[error("the occurrence this write names is not one the calendar holds")]
    StaleOccurrence,
    /// A wall clock the mapping tried to render does not exist as a calendar
    /// value (an out-of-range date).
    #[error("{0}")]
    Unrepresentable(String),
}

/// A new event to create, the user-chosen half of the engine's
/// [`EventDraft`]: the core mints the `UID` and the stamp beside it.
pub(crate) struct NewEvent<'a> {
    /// The calendar the event lands in.
    pub calendar: engine_api::CalendarId,
    /// The title.
    pub summary: &'a str,
    /// The start. For an all-day event this is the `date` form; the end is then
    /// exclusive (a one-day event on the 1st ends on the 2nd).
    pub start: &'a CalendarDateTime,
    /// The end, in the same form as the start.
    pub end: &'a CalendarDateTime,
    /// The description, if any.
    pub description: Option<&'a str>,
    /// The location, if any.
    pub location: Option<&'a str>,
    /// How the event repeats, or `None` for a one-off.
    pub recurrence: Option<&'a RepeatRule>,
}

/// Builds the engine's [`EventDraft`] for one create intent, under the `uid`
/// and `stamp` the core mints at dispatch and the week start the persisted
/// preference names: the host cannot mint a valid `Uid` and should not be
/// asked to.
///
/// # Errors
///
/// Returns [`CalendarEditError::Unrepresentable`] when a repeat rule's end
/// cannot be stated in the event's own terms.
pub(crate) fn build_draft(
    new: &NewEvent<'_>,
    uid: Uid,
    stamp: UtcDateTime,
    week_start: engine_api::Weekday,
) -> Result<EventDraft, CalendarEditError> {
    let recurrence = new
        .recurrence
        .map(|rule| draft_recurrence(rule, new.start, week_start))
        .transpose()?;
    let mut draft = EventDraft::new(
        new.calendar.clone(),
        uid,
        new.summary,
        new.start.clone(),
        new.end.clone(),
        stamp,
    );
    if let Some(recurrence) = recurrence {
        draft = draft.repeating(recurrence);
    }
    if let Some(notes) = new.description.filter(|notes| !notes.is_empty()) {
        draft = draft.description(notes);
    }
    if let Some(place) = new.location.filter(|place| !place.is_empty()) {
        draft = draft.location(place);
    }
    Ok(draft)
}

/// Builds the engine's [`DraftRecurrence`] for a series whose event starts at
/// `start`, under the week start the editor writes its rules with.
///
/// The one conversion is the [`RepeatEnd::OnDate`] rule: the named date is
/// inclusive, so the rule's `UNTIL` wall clock is the **last second of that
/// date**, which includes an occurrence any time that day in either a timed or
/// an all-day series. A zoned event's `UNTIL` must then reach iCalendar as an
/// instant (RFC 5545 §3.3.10), and no adapter carries tzdata to resolve one,
/// so the instant is resolved here and travels with the rule.
fn draft_recurrence(
    rule: &RepeatRule,
    start: &CalendarDateTime,
    week_start: engine_api::Weekday,
) -> Result<DraftRecurrence, CalendarEditError> {
    let engine_rule = rebuild_rule(rule, week_start)?;
    let RecurrenceBound::Until(until) = &engine_rule.bound else {
        return Ok(DraftRecurrence::new(engine_rule));
    };
    let bound = in_event_form(start, *until)?;
    let instant = resolve_instant(&bound)
        .map_err(|err| CalendarEditError::Unrepresentable(err.to_string()))?;
    Ok(match instant {
        Some(instant) => DraftRecurrence::ending_at(engine_rule, instant),
        None => DraftRecurrence::new(engine_rule),
    })
}

/// The last representable wall clock on `date`: the inclusive end of a repeat
/// rule that names it.
fn day_end(date: CalendarDate) -> Result<LocalDateTime, CalendarEditError> {
    LocalDateTime::new(date.year(), date.month(), date.day(), 23, 59, 59)
        .map_err(|_| CalendarEditError::Unrepresentable("the repeat end is not a real date".into()))
}

/// Builds the `(PatchTarget, EventPatch)` pair one edit intent submits through
/// the engine's patch verb, rendered in the stored event's own form so the
/// properties this model does not carry (the alarms, the attendees, another
/// client's `X-` extensions) survive: the adapter patches the stored payload
/// rather than rebuilding it.
///
/// # Errors
///
/// Returns [`CalendarEditError`] when the edit asks for something that must
/// not be written: a recurrence change on an occurrence or on a rule too rich
/// for the editor, a series shift from an occurrence without both clocks, or a
/// clock that will not render in the event's form.
pub(crate) fn build_patch(
    event: &Event,
    edit: &EventEdit,
    stamp: UtcDateTime,
    week_start: engine_api::Weekday,
) -> Result<(PatchTarget, EventPatch), CalendarEditError> {
    let mut patch = EventPatch::new(stamp);
    if let Some(title) = &edit.title {
        patch = patch.summary(title);
    }
    // Clocks read from one occurrence, on an edit meant for the series, are a
    // shift rather than the series' own times: writing them straight through
    // would move the series' start to that occurrence and every earlier one
    // would stop existing.
    let (start, end) = match (edit.times_from_occurrence, edit.occurrence) {
        (Some(read_from), None) => {
            let (Some(edited_start), Some(edited_end)) = (edit.start, edit.end) else {
                return Err(CalendarEditError::ShiftNeedsBothEdges);
            };
            let (start, end) = series_bounds_after(event, read_from, edited_start, edited_end)?;
            (Some(start), Some(end))
        }
        _ => (edit.start, edit.end),
    };
    // An instance patch states the occurrence's own times: the engine's
    // CalDAV path refuses a first-touch override without both, because the
    // copy would inherit the master's slot. A host that states neither is
    // making a content-only edit, so both come back unchanged from the stored
    // event; one edge without the other is neither and refuses typed.
    let (start, end) = match (edit.occurrence, start, end) {
        (Some(original), None, None) => {
            let (start, end) = occurrence_bounds(event, original)?;
            (Some(start), Some(end))
        }
        (Some(_), Some(_), Some(_)) | (None, _, _) => (start, end),
        (Some(_), _, _) => return Err(CalendarEditError::OccurrenceNeedsBothEdges),
    };
    if let Some(start) = start {
        patch = patch.start(in_event_form(&event.start, start)?);
    }
    if let Some(end) = end {
        patch = patch.end(in_event_form(&event.start, end)?);
    }
    if let Some(notes) = &edit.notes {
        patch = if notes.is_empty() {
            patch.clear_description()
        } else {
            patch.description(notes)
        };
    }
    if let Some(place) = &edit.location {
        patch = if place.is_empty() {
            patch.clear_location()
        } else {
            patch.location(place)
        };
    }
    if let Some(change) = &edit.recurrence {
        if edit.occurrence.is_some() {
            return Err(CalendarEditError::RuleOnOccurrence);
        }
        patch = match change {
            RecurrenceChange::Clear => patch.clear_recurrence(),
            RecurrenceChange::Set(rule) => {
                // The surface can only have seen what the rebuild described, so
                // a stored rule it could not describe is a rule it must not
                // write back; checked again here because a write must not
                // trust its caller.
                if !rule_is_editable(event.recurrence.as_ref(), week_start) {
                    return Err(CalendarEditError::RuleTooRich);
                }
                patch.recurrence(draft_recurrence(rule, &event.start, week_start)?)
            }
        };
    }
    let target = match edit.occurrence {
        Some(original) => PatchTarget::Instance(event_occurrence(event, original)?),
        None => PatchTarget::Series,
    };
    Ok((target, patch))
}

/// Builds the engine's [`EventDeletion`] for a delete intent: the whole series,
/// or the one occurrence that originally started at `occurrence`. The two are
/// different requests and only the user knows which they meant.
///
/// # Errors
///
/// Returns [`CalendarEditError::Unrepresentable`] when the occurrence's wall
/// clock will not render in the stored event's form.
pub(crate) fn build_deletion(
    event: &Event,
    occurrence: Option<LocalDateTime>,
    stamp: UtcDateTime,
) -> Result<EventDeletion, CalendarEditError> {
    Ok(match occurrence {
        None => EventDeletion::of(event),
        Some(original) => {
            EventDeletion::occurrence(event, event_occurrence(event, original)?, stamp)
        }
    })
}

/// Whether the stored `Recurrence` is one this editor can describe in full
/// through [`RepeatRule`]: no rule at all (a one-off), or exactly one rule
/// that survives a describe/rebuild round trip under `week_start`, the week
/// start the editor writes its rules with. Anything else stays read-only.
///
/// The fit is decided by the **round trip**, not by a checklist of the fields
/// this version knows about: the stored rule is projected onto
/// [`RepeatRule`] (dropping whatever the editor cannot hold) and rebuilt
/// through [`rebuild_rule`], the same construction a `Set` would write. A
/// checklist would decide the same question by hand and be wrong the day the
/// engine gains a field: a rule carrying it would look editable, the editor
/// would seed itself from a projection missing that field, and saving would
/// write the rule back without it. Silently, on the user's real series. The
/// round trip cannot make that mistake: a field this module does not carry is
/// a field the rebuilt rule does not match on, so the rule degrades to
/// read-only instead of being quietly rewritten. The week start joins the
/// round trip the same way: a rule the editor would rebuild identically under
/// the setting's day is editable, and one whose anchor a save would move
/// (a `WKST` the setting does not name, an exclusion rule, a skip policy, a
/// rule measured in hours) refuses by coming back different, not by being
/// named.
pub(crate) fn rule_is_editable(
    recurrence: Option<&engine_api::Recurrence>,
    week_start: engine_api::Weekday,
) -> bool {
    let Some(recurrence) = recurrence else {
        return true;
    };
    // More than one rule, or a rule whose instances are subtracted, is a set
    // an editor built from one frequency cannot state.
    let [rule] = recurrence.rules.as_slice() else {
        return false;
    };
    if !recurrence.excluded_rules.is_empty() {
        return false;
    }
    describe_rule(rule)
        .and_then(|simple| rebuild_rule(&simple, week_start).ok())
        .is_some_and(|rebuilt| rebuilt == *rule)
}

/// Projects one stored rule onto [`RepeatRule`], dropping whatever the
/// editor cannot state. Lossy by design; [`rule_is_editable`] is what decides
/// whether anything was lost, by rebuilding and comparing.
fn describe_rule(rule: &RecurrenceRule) -> Option<RepeatRule> {
    // A repeat measured in hours or finer is not a calendar repeat any
    // editor offers, so it is not one this editor pretends to describe.
    if matches!(
        rule.frequency,
        Frequency::Hourly | Frequency::Minutely | Frequency::Secondly
    ) {
        return None;
    }
    // A weekday pinned to its nth occurrence in the period is a part the
    // editor does not carry at all, rather than one it drops: naming every
    // Monday is not naming the fourth.
    if rule.by_day.iter().any(|day| day.nth_of_period.is_some()) {
        return None;
    }
    Some(RepeatRule {
        frequency: rule.frequency,
        interval: rule.interval.get(),
        weekdays: rule.by_day.iter().map(|day| day.day).collect(),
        end: match &rule.bound {
            RecurrenceBound::Unbounded => RepeatEnd::Never,
            RecurrenceBound::Count(count) => RepeatEnd::AfterCount(count.get()),
            RecurrenceBound::Until(until) => {
                RepeatEnd::OnDate(CalendarDate::new(until.year(), until.month(), until.day()).ok()?)
            }
        },
    })
}

/// Rebuilds the engine rule a [`RepeatRule`] stands for, under `week_start`:
/// the one construction shared by the create path and the editability round
/// trip, so a rule the editor writes can never differ from a rule the editor
/// was allowed to edit.
///
/// A rule that names no weekday anchors to the week boundary rather than to a
/// day, so it carries `week_start` as its `WKST`; a rule that names its
/// weekdays keeps the engine's own default, because its occurrences follow
/// the named days and a week start would state nothing about them.
fn rebuild_rule(
    rule: &RepeatRule,
    week_start: engine_api::Weekday,
) -> Result<RecurrenceRule, CalendarEditError> {
    let interval = NonZeroU32::new(rule.interval)
        .ok_or_else(|| CalendarEditError::Unrepresentable("a zero interval".to_owned()))?;
    let mut engine_rule = RecurrenceRule::new(rule.frequency);
    engine_rule.interval = interval;
    if rule.weekdays.is_empty() {
        engine_rule.first_day_of_week = week_start;
    }
    engine_rule.by_day = rule
        .weekdays
        .iter()
        .map(|day| engine_api::NDay {
            day: *day,
            nth_of_period: None,
        })
        .collect();
    engine_rule.bound = match rule.end {
        RepeatEnd::Never => RecurrenceBound::Unbounded,
        RepeatEnd::AfterCount(count) => RecurrenceBound::Count(
            NonZeroU32::new(count)
                .ok_or_else(|| CalendarEditError::Unrepresentable("a zero count".to_owned()))?,
        ),
        // The inclusive-date rule: the day named is the last one that recurs,
        // so the bound is its final second rather than its midnight, which
        // would cut the day's own occurrence.
        RepeatEnd::OnDate(date) => RecurrenceBound::Until(day_end(date)?),
    };
    Ok(engine_rule)
}

/// The engine's own spelling of a settings weekday: the WKST mapping, one
/// place, so a first day stated to the settings surface and a repeat rule's
/// week start spell one day the same way.
pub(crate) fn weekday_of(day: crate::protocol::WeekdayWire) -> engine_api::Weekday {
    match day {
        crate::protocol::WeekdayWire::Mo => engine_api::Weekday::Mo,
        crate::protocol::WeekdayWire::Tu => engine_api::Weekday::Tu,
        crate::protocol::WeekdayWire::We => engine_api::Weekday::We,
        crate::protocol::WeekdayWire::Th => engine_api::Weekday::Th,
        crate::protocol::WeekdayWire::Fr => engine_api::Weekday::Fr,
        crate::protocol::WeekdayWire::Sa => engine_api::Weekday::Sa,
        crate::protocol::WeekdayWire::Su => engine_api::Weekday::Su,
    }
}

/// The engine's [`Occurrence`] for one original start, named the way every
/// transport addresses one: the start in the series' own form, plus the same
/// moment as an instant where the zone admits one (Google alone needs it, but
/// resolving it here costs nothing and keeps the adapter from refusing).
fn event_occurrence(
    event: &Event,
    original: LocalDateTime,
) -> Result<Occurrence, CalendarEditError> {
    let start = in_event_form(&event.start, original)?;
    let instant = resolve_instant(&start)
        .map_err(|err| CalendarEditError::Unrepresentable(err.to_string()))?;
    Ok(match instant {
        Some(instant) => Occurrence::at(start, instant),
        None => Occurrence::starting(start),
    })
}

/// Renders a wall clock in the same **form** the stored event uses: zoned in
/// its own zone, floating, or an all-day date. This is what keeps the engine's
/// form guard from ever firing in practice.
fn in_event_form(
    current: &CalendarDateTime,
    local: LocalDateTime,
) -> Result<CalendarDateTime, CalendarEditError> {
    Ok(match current {
        CalendarDateTime::Date(_) => CalendarDateTime::Date(
            CalendarDate::new(local.year(), local.month(), local.day())
                .map_err(|err| CalendarEditError::Unrepresentable(err.to_string()))?,
        ),
        CalendarDateTime::Floating(_) => CalendarDateTime::Floating(local),
        CalendarDateTime::Zoned { zone, .. } => CalendarDateTime::Zoned {
            local,
            zone: zone.clone(),
        },
    })
}

/// Resolves a wall clock in a named zone to an instant, for the tests' and the
/// invite machine's need to place a meeting on the UTC timeline.
pub(crate) fn instant_of(
    value: &CalendarDateTime,
    zone: &TimeZoneId,
) -> Result<UtcDateTime, CalendarEditError> {
    resolve_instant_in(value, zone)
        .map_err(|err| CalendarEditError::Unrepresentable(err.to_string()))
}

#[cfg(test)]
#[path = "../calendar_edit_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../calendar_edit_rule_tests.rs"]
mod rule_tests;
