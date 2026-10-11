// SPDX-License-Identifier: GPL-3.0-only
//! The calendar view-model: an immutable snapshot a host pulls after a
//! `calendar` signal and renders as the calendar manager, the agenda, and the
//! day, work-week, and month grids.
//!
//! Pure projection over the engine's calendar state, like the mailbox list
//! beside it: state lives in the engine, the app rebuilds this snapshot from
//! the engine's calendar and occurrence reads, and every surface shares one
//! definition of what is on screen. The rows carry the joined view the engine
//! itself never assembles in one read: a calendar's colour and rights, and an
//! occurrence's instants beside its master's title and markers.

use engine_api::{AccountId, CalendarId, LocalDateTime, ProviderKey, UtcDateTime};
use serde::{Deserialize, Serialize};

/// An immutable calendar snapshot for a surface to render: the calendar
/// manager set, the visible window's occurrences, the write states, and the
/// now line data.
///
/// Empty before any calendar state exists: a cold host renders an empty
/// calendar, not a zeroed one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarSnapshot {
    /// Every account's calendars, for the calendar manager, one row per
    /// calendar the store holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calendars: Vec<CalendarRow>,
    /// The occurrences overlapping the visible window, ascending by start:
    /// what the grids and the agenda lay out. Each row is the joined view of
    /// one occurrence (its instants) and its master event (its content).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occurrences: Vec<OccurrenceRow>,
    /// The accounts with a calendar write in flight or unconfirmed: what a
    /// surface draws its write indicator from. No row means nothing is being
    /// written and nothing needs confirming.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_states: Vec<CalendarWriteRow>,
    /// What the now line draws from, once the app has a clock reading for the
    /// view. `None` on the default snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_data: Option<NowLineData>,
}

/// One calendar a user can see, colour, and toggle: the row the calendar
/// manager lists and the grids colour their blocks from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarRow {
    /// The account whose calendar this is: a calendar key is unique only
    /// within its account, so the row carries the account an action on it
    /// routes to.
    pub account: AccountId,
    /// The calendar's key, unique within `account`.
    pub id: CalendarId,
    /// The display name.
    pub name: String,
    /// The display colour, a CSS colour name or `#hex`, exactly as the
    /// provider stated it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Whether the calendar's events are currently drawn.
    pub visible: bool,
    /// Whether the account may write to this calendar (the engine's
    /// `may_write` right): a surface hides its edit affordances when false.
    pub can_write: bool,
    /// Whether a new event lands here unless the user picks another calendar.
    pub is_default: bool,
}

/// One materialized occurrence, joined to the master event that carries its
/// content.
///
/// The engine's store rows carry *when*; the master event carries *what*. This
/// row is the join: the occurrence's resolved instants beside the master's
/// title, calendar, and markers, plus the occurrence token a drag or a
/// one-occurrence write hands back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceRow {
    /// The account whose store holds the master event: an action on the row
    /// routes to it, because two accounts can mint the same event key.
    pub account: AccountId,
    /// The master event's provider key: what an open, edit, or delete names.
    pub event: ProviderKey,
    /// The calendar the event belongs to: the key for its colour and its
    /// visibility.
    pub calendar: CalendarId,
    /// The event's title, as the master carries it (empty when the event has
    /// none; a surface renders its own placeholder rather than inventing one
    /// here).
    pub title: String,
    /// The occurrence's start, resolved to a UTC instant by the engine's
    /// expansion.
    pub start: UtcDateTime,
    /// The occurrence's end (exclusive), resolved the same way.
    pub end: UtcDateTime,
    /// Whether the master event is all-day (a date-only value).
    pub all_day: bool,
    /// Whether the master event repeats: the marker a grid draws its repeat
    /// glyph from.
    pub recurring: bool,
    /// Whether this instance is an override the series carries (a
    /// `recurrence_id` on the store's row): an instance someone moved or
    /// retitled on its own.
    pub overridden: bool,
    /// This occurrence's **original** start, as a wall clock in the event's
    /// own zone: the token a drag or a one-occurrence delete hands back.
    /// Absent when the event does not repeat.
    ///
    /// Opaque to the host: an identifier for "the Tuesday you dragged", never
    /// a time to compute with. The core mints it and the core reads it back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrence_start: Option<LocalDateTime>,
    /// How the instants were resolved: what a grid may promise about the times
    /// it draws.
    pub accuracy: OccurrenceAccuracy,
}

/// How an occurrence row's instants were resolved, per the master event's time
/// form. Crosses as a snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceAccuracy {
    /// A zoned (or UTC) event resolved through its own zone: the instants are
    /// exact for everyone.
    Zoned,
    /// A floating event resolved through the display zone: exact only while
    /// the zone the snapshot was expanded in is the one it is drawn in.
    Floating,
    /// An all-day event: the instants are the display zone's local midnights,
    /// a drawing hint rather than a scheduled time.
    AllDay,
}

/// One account's calendar write, as the surface's write indicator shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarWriteRow {
    /// The account whose write the row reports on.
    pub account: AccountId,
    /// Where the write has got to.
    pub state: CalendarWriteState,
}

/// Where a calendar write has got to. Crosses as a snake_case word.
///
/// Two states, because those are the two a person can act on: a write settling
/// (the spinner) and a write whose local view could not be confirmed (the
/// warning with a retry). A write that settled cleanly crosses no row at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarWriteState {
    /// A calendar write is in flight.
    Writing,
    /// The last write could not be confirmed: the server may hold it or not,
    /// so the retry is a **re-read**, never the same write issued again, and
    /// the row stays until a refresh or the next write resolves it. A failed
    /// confirmation is not a rejected change.
    FailedWithRetry,
}

/// What the now line draws from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowLineData {
    /// The instant the snapshot was built at: the now line's position at
    /// publish. A host advances the line with its own clock between publishes
    /// rather than waiting for the next one.
    pub now: UtcDateTime,
    /// The IANA id of the zone the grids lay their days out in: the expansion
    /// zone the floating and all-day instants were resolved through, and the
    /// zone a host localises each row against. The display zone is always an
    /// IANA id, so it crosses as the bare name (an event's own zone may be a
    /// server-defined one, but the view's never is).
    pub zone: String,
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
