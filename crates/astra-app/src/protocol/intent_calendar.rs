// SPDX-License-Identifier: GPL-3.0-only
//! The calendar half of the inbound protocol ([`CalendarIntent`]), which
//! [`Intent::Calendar`](super::Intent::Calendar) carries.
//!
//! Split from [`super::intent`] to keep each file under the 500-line limit,
//! following [`ComposeIntent`](super::ComposeIntent). The family reads as a unit
//! for the same reason: the view verbs and the write verbs answer one surface,
//! and every field is engine-shaped, in the engine's own types, so the dispatch
//! maps onto the engine's calendar reads and writes without a second
//! protocol vocabulary in between.
//!
//! Two rules the engine forces and this family encodes:
//!
//! - **A write names an occurrence by its original start, never by a synthetic
//!   key.** An occurrence of a recurring series is not a stored object of its
//!   own; the engine's whole-series-vs-one-occurrence question has no safe
//!   default, so the drag, edit, and delete intents carry `Option<LocalDateTime>`
//!   and the surface asks the user which they meant before filling it in.
//! - **A time the host states is a wall clock, never a resolved instant.** The
//!   engine rejects an edit that would change an event's time form (a zoned
//!   event to a UTC instant, an all-day event to a timed one), so the edit and
//!   drag intents speak the event's own clock and leave the resolution to the
//!   core, which holds the tzdata.

use engine_api::{CalendarDate, CalendarDateTime, Frequency, LocalDateTime, UtcDateTime, Weekday};
use serde::{Deserialize, Serialize};

use crate::reference::{CalendarRef, EventRef, MessageRef};

/// A repeat rule as the host can state it: the editor's subset of the engine's
/// full recurrence model, in the engine's own types.
///
/// A rule richer than this (a non-Gregorian `rscale`, an nth-of-period weekday,
/// a by-month-day pattern) is not expressible by an intent; the core refuses to
/// replace a stored rule the editor cannot describe in full rather than
/// rewriting it from a partial picture, and the surface offers no edit for one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatRule {
    /// The base frequency.
    pub frequency: Frequency,
    /// How many periods between instances; never zero, `1` for every period.
    pub interval: u32,
    /// The named weekdays, empty when the rule takes them from its start.
    pub weekdays: Vec<Weekday>,
    /// When the series stops.
    pub end: RepeatEnd,
}

/// When a repeat rule stops. Crosses as a snake_case word, alone or with its
/// bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatEnd {
    /// It never stops.
    Never,
    /// On a calendar date in the event's own terms, inclusive.
    ///
    /// The recorded rule for the date's conversion to the engine's `UNTIL`
    /// bound: the wall clock is the **final second of the named date**,
    /// rendered in the form of the event's own start, so an occurrence any
    /// time that day recurs in either a timed or an all-day series. A zoned
    /// event's bound resolves through its own zone to the instant RFC 5545
    /// §3.3.10 demands; a floating or all-day one stays a wall clock, which
    /// has no instant to resolve to. The core does the conversion, never
    /// the host.
    OnDate(CalendarDate),
    /// After a fixed number of instances, counting the first.
    AfterCount(u32),
}

/// What an edit does to an event's repeat rule. Three states with
/// `Option<RecurrenceChange>`: leaving recurrence out of an edit keeps the
/// series exactly as it was, which is not the same as turning a repeating
/// event into a single one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceChange {
    /// Replace the rule, or give a one-off event its first one.
    Set(RepeatRule),
    /// Stop repeating: every occurrence but the first goes.
    Clear,
}

/// Which edges of an event a drag moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventEdge {
    /// The block itself was dragged: both edges move together, so the duration
    /// is preserved exactly.
    Whole,
    /// The start edge was dragged: the event starts earlier or later and ends
    /// where it did.
    Start,
    /// The end edge was dragged: the event ends earlier or later and starts
    /// where it did.
    End,
}

/// One drag of an event on the grid, in the currency the grid speaks.
///
/// `days` and `minutes` are **wall-clock** offsets, both signed and both
/// applied together: a block dragged from Monday 09:00 to Tuesday 08:30 is
/// `days: 1, minutes: -30`. The same numbers apply in the event's own zone and
/// in the display zone, so the host snaps its drop to the quarter hour and
/// sends the delta without converting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDrag {
    /// Which edges moved.
    pub edge: EventEdge,
    /// Whole days the dragged edge or edges move by.
    pub days: i32,
    /// Minutes within the day the dragged edge or edges move by. Ignored for an
    /// all-day event, which has no clock to move along.
    pub minutes: i32,
    /// Which occurrence of a recurring event was dragged, named by its
    /// **original** start as a wall clock in the event's own zone (the
    /// occurrence token the snapshot's rows carry).
    ///
    /// `None` moves the **whole series**. There is no default and there must
    /// not be one: dragging one Tuesday standup is not the same as rewriting
    /// every Tuesday to eternity, and only the user knows which they meant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<LocalDateTime>,
}

/// An edit to a **stored** event, expressed in the event's **own wall clock**.
///
/// A stored event is zoned, floating, or all-day, and an edit must not change
/// which: the engine's patcher rejects a form change outright, so an edit says
/// what the clock on the wall reads and the core renders it in whatever form
/// the stored event already has. A host editing in a display zone other than
/// the event's own must convert to the event's zone first; a drag does not have
/// to, for the reason on [`EventDrag`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEdit {
    /// The new title, or `None` to leave it alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new start, as a wall clock in the event's own zone (its date, if
    /// all-day).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<LocalDateTime>,
    /// The new end, same terms. For an all-day event the end date is
    /// **exclusive**: a one-day event on the 1st ends on the 2nd.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<LocalDateTime>,
    /// The new description, three-state: `None` leaves it alone, `Some("")`
    /// clears it, `Some(text)` sets it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// The new location, same three-state semantics as `notes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// What happens to the repeat rule: `None` leaves the series alone,
    /// [`Set`](RecurrenceChange::Set) replaces the rule,
    /// [`Clear`](RecurrenceChange::Clear) makes the event a single one.
    ///
    /// Only a **series** edit may carry one: a rule belongs to the series, and
    /// one occurrence is an instance of a rule rather than a holder of one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<RecurrenceChange>,
    /// Which occurrence of a recurring event this edits, named by its
    /// **original** start. `None` edits the **whole series**; there is no
    /// default on purpose, for the reason on [`EventDrag::occurrence`].
    ///
    /// Editing a single occurrence states `start` **and** `end` (the series
    /// master's are the first occurrence's times, not this one's). A host
    /// that states neither is making a content-only edit: the app fills both
    /// unchanged from the stored event, whose zone only it holds. One edge
    /// without the other is neither and is refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<LocalDateTime>,
    /// The occurrence `start` and `end` were **read from**, when this edit is
    /// scoped to the series but its editor was opened on one occurrence.
    ///
    /// An editor opened on one occurrence shows that occurrence's times, so
    /// writing them onto the master would move the series' start to that
    /// occurrence and every earlier one would stop existing. Naming where the
    /// clocks came from turns the edit into the *shift* the user made, applied
    /// to the series' own clock. Ignored when `occurrence` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times_from_occurrence: Option<LocalDateTime>,
}

/// The answer a user can give to an invitation.
///
/// The engine's own `RsvpResponse` in substance and in value, restated here so
/// the whole inbound wire crosses as snake_case words: the engine's type
/// serialises its variants verbatim, and one CamelCase word inside an otherwise
/// snake_case family is a trap for a host that writes its literals by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RsvpAnswer {
    /// Yes.
    Accepted,
    /// Maybe.
    Tentative,
    /// No.
    Declined,
}

/// What the calendar surface asks the runtime to do.
///
/// The reads and writes share one family because they answer one surface: the
/// view verbs position the grids, the write verbs change the diary the grids
/// show, and every write's outcome the snapshot's write states carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CalendarIntent {
    /// Sync the account's calendars and refresh the calendar snapshot.
    Refresh,
    /// Move the visible window: `start` is the UTC instant the window opens at
    /// and `days` its length. The core widens the engine's expansion horizon to
    /// cover it, because re-syncing alone never materializes occurrences a
    /// window first asks for.
    SetRange {
        /// The UTC instant the window opens at (the host derives it from the
        /// anchor date in the display zone).
        start: UtcDateTime,
        /// How many days the window spans.
        days: u16,
    },
    /// Which calendars' events are drawn: the toggle set of the calendar
    /// manager. An empty set hides every calendar's events; the ids are
    /// references because a calendar key is unique only within its account.
    SetVisibleCalendars {
        /// The calendars whose events are drawn.
        ids: Vec<CalendarRef>,
    },
    /// Create an event, in the engine's own create shape: the fields of the
    /// engine's `EventDraft` that are the user's to choose. The core mints the
    /// event's cross-system `UID`, the write stamp, and the idempotency key;
    /// the adapter serialises the event in the account's protocol.
    CreateEvent {
        /// The calendar the event lands in, bound to its owning account.
        calendar: CalendarRef,
        /// The title.
        summary: String,
        /// The start. For an all-day event this is the `date` form; the end is
        /// then **exclusive** (a one-day event on the 1st ends on the 2nd).
        start: CalendarDateTime,
        /// The end, in the same form as the start.
        end: CalendarDateTime,
        /// The description, if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// The location, if any. A create is the one write that can set it from
        /// nothing; an edit reshapes it through [`EventEdit::location`].
        #[serde(skip_serializing_if = "Option::is_none")]
        location: Option<String>,
        /// How the event repeats, or `None` for a one-off.
        #[serde(skip_serializing_if = "Option::is_none")]
        recurrence: Option<RepeatRule>,
    },
    /// Edit a stored event: the core re-reads the master the reference names,
    /// renders the edit in the stored event's own form, and submits it through
    /// the engine's patch verb, so properties this model does not carry (the
    /// alarms, the attendees, another client's extensions) survive.
    UpdateEvent {
        /// The event to edit, bound to its owning account.
        event: EventRef,
        /// What the edit changes, in the event's own wall clock.
        edit: EventEdit,
    },
    /// Move or resize an event by a drag: the core applies the delta to the
    /// event's own clock, clamps a resize so the event keeps at least a quarter
    /// of an hour (a whole day, when all-day), and submits the result through
    /// the same patch path an edit takes.
    MoveEvent {
        /// The event to move, bound to its owning account.
        event: EventRef,
        /// The gesture, in the grid's own currency.
        drag: EventDrag,
    },
    /// Delete an event, or one occurrence of it. The two are different
    /// requests and only the user knows which they meant; `None` deletes the
    /// whole series, a wall clock names the one occurrence that originally
    /// started then (the occurrence token the snapshot's rows carry).
    DeleteEvent {
        /// The event to delete, bound to its owning account.
        event: EventRef,
        /// The one occurrence to remove, leaving the rest of the series.
        #[serde(skip_serializing_if = "Option::is_none")]
        occurrence: Option<LocalDateTime>,
    },
    /// Answer a meeting invitation: the message the reading pane is showing,
    /// and the answer. The core resolves the stored event by the invitation's
    /// `UID` and the attendee line the invitation **matched** (an alias, not
    /// necessarily the account's primary address), then submits through the
    /// engine's RSVP verb, the one calendar write that tells the organiser.
    RespondToInvitation {
        /// The invitation message, bound to its owning account.
        message: MessageRef,
        /// The answer.
        response: RsvpAnswer,
        /// A note for the organiser, offered only where the account's transport
        /// can carry one; a transport that cannot refuses the write rather than
        /// dropping the note silently.
        #[serde(skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
        /// Whether the organiser is told. `true` is the RFC 5546 default; a
        /// transport whose server sends the reply the moment the status changes
        /// refuses a `false` rather than pretending to hold it back.
        notify_organizer: bool,
    },
    /// Answer the reply-delivery-failure prompt: whether to email the
    /// organiser the iTIP `REPLY` the calendar server reported it could not
    /// deliver, and whether the account learns the decision. The pending
    /// prompt (the most recently raised one, the question a surface is
    /// showing) clears either way; the answer itself already landed on the
    /// calendar long before the question arose, so nothing here re-writes it.
    AnswerReplyPrompt {
        /// Whether to send the reply by mail, through the account's submit
        /// path. `false` dismisses the question without sending: the organiser
        /// is not told, which is a decision the user is entitled to make.
        send: bool,
        /// Whether the account learns the decision for next time: a
        /// remembered send mails every future reply this account's server
        /// fails to deliver, with no question asked; a remembered refusal
        /// never asks and never mails. Absent (`false`), the question is
        /// asked again the next time a delivery fails.
        remember: bool,
        /// The subject line the reply carries, when the host composed a
        /// localised one; absent, the reply quotes the invitation's own
        /// subject back, prefixed the RFC 5322 way.
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_subject: Option<String>,
    },
    /// Move the grids' navigation anchor to `date` (the jump-to-today and the
    /// date picker). The anchor positions the view; the window itself is
    /// [`SetRange`](Self::SetRange)'s.
    SetDate {
        /// The date the grids navigate to.
        date: CalendarDate,
    },
}
