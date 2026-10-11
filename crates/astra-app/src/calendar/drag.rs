// SPDX-License-Identifier: GPL-3.0-only
//! The drag maths: a gesture on the grid turned into the edit the engine
//! takes, in the event's own wall clock. Ported from mail-calendar's
//! `calendar_drag`, whose rules bind here:
//!
//! - **A drag is wall-clock arithmetic, not elapsed time.** Dragging an event
//!   across a spring-forward boundary keeps it at 10:00 rather than landing it
//!   at 11:00, which is what the grid shows and what the user meant, so the
//!   civil-day maths lives here rather than in the host.
//! - **A resize clamps, never refuses.** A gesture cannot produce a block
//!   shorter than a quarter of an hour (a whole day, when all-day), because a
//!   calendar that refuses the gesture leaves the user holding a block that
//!   will not shrink, with nothing on screen to say why.
//! - **The offsets are bounded.** The deltas cross the IPC, and the civil-date
//!   conversion saturates outside representable calendar time, so a limit that
//!   rejects is better than a conversion that cannot say what it produced.

use engine_api::{
    resolve_instant, to_local, CalendarDateTime, Event, LocalDateTime, TimeZoneId, UtcDateTime,
};

use crate::protocol::{EventDrag, EventEdge, EventEdit};

use super::edit::CalendarEditError;

/// The shortest an event may be left by a resize, in minutes: the same floor a
/// grid gives a block so it stays tappable.
const MIN_EVENT_MINUTES: i64 = 15;

/// Minutes in a day. Daylight saving cannot change this here: the drag
/// arithmetic is wall-clock.
const DAY_MINUTES: i64 = 24 * 60;

/// The furthest a single drag may carry an event, in days. A gesture cannot
/// produce anything near this; the bound exists because the offsets cross the
/// IPC and the civil-date conversion below panics outside representable
/// calendar time, so a limit that rejects is better than a conversion that
/// aborts the process.
const MAX_DRAG_DAYS: i64 = 366;

/// The drag's edit, in the event's own wall clock: the delta applied to
/// whichever edges the gesture named, a resize clamped so the event keeps at
/// least a quarter of an hour (a whole day, when all-day), and every other
/// property left alone.
///
/// # Errors
///
/// Returns [`CalendarEditError::DragTooFar`] beyond the bound, and
/// [`CalendarEditError::Unrepresentable`] when the event's own bounds or the
/// shifted result are not representable.
pub(crate) fn apply_drag(event: &Event, drag: &EventDrag) -> Result<EventEdit, CalendarEditError> {
    let (days, minutes) = bounded(drag, event.start.is_all_day())?;
    let (start, end) = own_bounds(event)?;
    let (start, end) = match drag.edge {
        // Both edges by the same delta: the duration comes out bit-identical,
        // so there is nothing to clamp.
        EventEdge::Whole => (shift(start, days, minutes)?, shift(end, days, minutes)?),
        // A dragged start may not pass its end, so it clamps towards the edge
        // it did not touch, and only that edge moves.
        EventEdge::Start => (
            earlier(shift(start, days, minutes)?, latest_start(end, event)?),
            end,
        ),
        EventEdge::End => (
            start,
            later(shift(end, days, minutes)?, earliest_end(start, event)?),
        ),
    };
    Ok(EventEdit {
        start: Some(start),
        end: Some(end),
        occurrence: drag.occurrence,
        ..EventEdit::default()
    })
}

/// The drag's `(days, minutes)` as `i64`, bounded, with an all-day event's
/// minutes dropped. An all-day event has no clock to move along, so a minute
/// component is not merely useless, it would round a date onto its neighbour;
/// dropped rather than rejected, because a host that sends one is asking for a
/// whole-day move.
fn bounded(drag: &EventDrag, all_day: bool) -> Result<(i64, i64), CalendarEditError> {
    let days = i64::from(drag.days);
    let minutes = if all_day { 0 } else { i64::from(drag.minutes) };
    let span = days + minutes.div_euclid(DAY_MINUTES);
    if span.abs() > MAX_DRAG_DAYS {
        return Err(CalendarEditError::DragTooFar(span));
    }
    Ok((days, minutes))
}

/// The event's own `(start, end)` as wall clocks in its own zone. An all-day
/// event's end is the exclusive midnight after its last day, so a one-day
/// event on the 1st comes back `(1st, 2nd)` and a whole-day resize is plain
/// arithmetic on that.
pub(crate) fn own_bounds(
    event: &Event,
) -> Result<(LocalDateTime, LocalDateTime), CalendarEditError> {
    let unrepresentable =
        || CalendarEditError::Unrepresentable("the event's own times are not representable".into());
    match &event.start {
        CalendarDateTime::Date(date) => {
            let start = LocalDateTime::new(date.year(), date.month(), date.day(), 0, 0, 0)
                .map_err(|_| unrepresentable())?;
            let days = i64::try_from(event.duration.days()).unwrap_or(1).max(1);
            Ok((start, shift(start, days, 0)?))
        }
        CalendarDateTime::Floating(local) | CalendarDateTime::Zoned { local, .. } => {
            Ok((*local, end_wall_clock(event).ok_or_else(unrepresentable)?))
        }
    }
}

/// The event's end as a wall clock in its own zone. A zoned start is resolved
/// through the tzdata and localised back, because a duration in absolute time
/// crosses a daylight-saving boundary as elapsed time, not as the same clock
/// the next day.
fn end_wall_clock(event: &Event) -> Option<LocalDateTime> {
    let duration = event.duration;
    let span = core::time::Duration::new(
        duration
            .days()
            .saturating_mul(86_400)
            .saturating_add(duration.seconds()),
        duration.nanoseconds(),
    );
    match &event.start {
        CalendarDateTime::Zoned { zone, .. } => {
            let end = resolve_instant(&event.start)
                .ok()
                .flatten()?
                .checked_add(span)?;
            to_local(end, zone).ok()
        }
        CalendarDateTime::Floating(local) => {
            let start = UtcDateTime::new(
                local.year(),
                local.month(),
                local.day(),
                local.hour(),
                local.minute(),
                local.second(),
            )
            .ok()?;
            to_local(start.checked_add(span)?, &TimeZoneId::utc()).ok()
        }
        CalendarDateTime::Date(_) => None,
    }
}

/// The latest a dragged **start** may land: the event's end, less the minimum
/// it must keep (a whole day, when all-day).
fn latest_start(end: LocalDateTime, event: &Event) -> Result<LocalDateTime, CalendarEditError> {
    if event.start.is_all_day() {
        shift(end, -1, 0)
    } else {
        shift(end, 0, -MIN_EVENT_MINUTES)
    }
}

/// The earliest a dragged **end** may land: the event's start, plus the
/// minimum it must keep.
fn earliest_end(start: LocalDateTime, event: &Event) -> Result<LocalDateTime, CalendarEditError> {
    if event.start.is_all_day() {
        shift(start, 1, 0)
    } else {
        shift(start, 0, MIN_EVENT_MINUTES)
    }
}

/// The series' own `(start, end)` after an edit whose clocks were read from
/// one occurrence: the user's shift applied to the series' own clock, with the
/// end following the edited **duration** rather than its own delta, so a
/// resize done on one occurrence resizes the series by the same amount.
pub(crate) fn series_bounds_after(
    event: &Event,
    read_from: LocalDateTime,
    edited_start: LocalDateTime,
    edited_end: LocalDateTime,
) -> Result<(LocalDateTime, LocalDateTime), CalendarEditError> {
    let (series_start, _) = own_bounds(event)?;
    let (days, minutes) = between(read_from, edited_start);
    let start = shift(series_start, days, minutes)?;
    let (span_days, span_minutes) = between(edited_start, edited_end);
    Ok((start, shift(start, span_days, span_minutes)?))
}

/// How far `to` is from `from`, in whole days and minutes: the same pair a
/// drag carries.
fn between(from: LocalDateTime, to: LocalDateTime) -> (i64, i64) {
    let (from_day, from_minute, _) = order(from);
    let (to_day, to_minute, _) = order(to);
    (to_day - from_day, to_minute - from_minute)
}

/// The earlier of two wall clocks.
fn earlier(a: LocalDateTime, b: LocalDateTime) -> LocalDateTime {
    if order(a) <= order(b) {
        a
    } else {
        b
    }
}

/// The later of two wall clocks.
fn later(a: LocalDateTime, b: LocalDateTime) -> LocalDateTime {
    if order(a) >= order(b) {
        a
    } else {
        b
    }
}

/// A wall clock as a comparable `(day, minute, second)`: the civil day number,
/// so the ordering is right across a month or year boundary that a
/// field-by-field compare would get wrong.
fn order(at: LocalDateTime) -> (i64, i64, i64) {
    (
        from_civil(at.year(), at.month(), at.day()),
        i64::from(at.hour()) * 60 + i64::from(at.minute()),
        i64::from(at.second()),
    )
}

/// `local` moved by `days` whole days and `minutes` minutes, on the wall
/// clock: civil-day arithmetic, so a move across a daylight-saving boundary
/// keeps the clock reading, and seconds ride along untouched.
fn shift(
    local: LocalDateTime,
    days: i64,
    minutes: i64,
) -> Result<LocalDateTime, CalendarEditError> {
    let total = i64::from(local.hour()) * 60 + i64::from(local.minute()) + minutes;
    let day =
        from_civil(local.year(), local.month(), local.day()) + days + total.div_euclid(DAY_MINUTES);
    let minute_of_day = total.rem_euclid(DAY_MINUTES);
    let date = date_at(day);
    LocalDateTime::new(
        date.year(),
        date.month(),
        date.day(),
        u8::try_from(minute_of_day / 60).expect("0..23"),
        u8::try_from(minute_of_day % 60).expect("0..59"),
        local.second(),
    )
    .map_err(|err| CalendarEditError::Unrepresentable(format!("the dragged time: {err}")))
}

/// Days from the civil epoch (1970-01-01) for a proleptic-Gregorian date:
/// Howard Hinnant's `days_from_civil`, exact for every date the engine can
/// represent and free of leap-year special cases.
fn from_civil(year: i32, month: u8, day: u8) -> i64 {
    let (y, m, d) = (i64::from(year), i64::from(month), i64::from(day));
    // Shift the year to start in March, so a leap day falls at the end of a
    // "year".
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The calendar date at a day number: the inverse of [`from_civil`]. Saturates
/// on a day number outside representable calendar time, which the drag bound
/// keeps unreachable.
fn date_at(day: i64) -> engine_api::CalendarDate {
    let z = day + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day_of_month = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    engine_api::CalendarDate::new(
        i32::try_from(year).unwrap_or(i32::MAX),
        u8::try_from(month).unwrap_or(12),
        u8::try_from(day_of_month).unwrap_or(28),
    )
    .unwrap_or_else(|_| engine_api::CalendarDate::new(1970, 1, 1).expect("the epoch"))
}

#[cfg(test)]
#[path = "../calendar_drag_tests.rs"]
mod tests;
