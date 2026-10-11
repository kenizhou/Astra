// SPDX-License-Identifier: GPL-3.0-only
//! The calendar snapshot rebuild: the engine's occurrence rows joined to their
//! masters, per the survey's contract. The grid read is
//! [`Engine::occurrences_in`] (a series renders once at its start if a grid
//! pages over master events), each row's master resolved through
//! [`Engine::events_by_keys`] (never [`Engine::events`], which decodes the
//! whole diary), and the join carries *when* from the row and *what* from the
//! master: the title, the calendar, the markers, the participation, and
//! whether the block may be dragged.
//!
//! Beside the join, the two mail-calendar rules the rows encode:
//!
//! - **A declined meeting leaves the calendar.** A `PARTSTAT=DECLINED` event
//!   is filtered out of the rebuild the way the engine's own reads filter
//!   tombstones, so a diary the user said no to never shows the meeting again.
//! - **Participation is a drawing question, `can_move` a writing one.** An
//!   event nobody was invited to is a commitment ([`Participation::Accepted`])
//!   because the user put it in their own diary; the same event is trivially
//!   draggable, while a meeting somebody else called on the same writable
//!   calendar is not, because silently re-timing another person's meeting is
//!   not what write access is for.

use std::collections::{HashMap, HashSet};

use astra_viewmodel::{
    CalendarRow, CalendarSnapshot, NowLineData, OccurrenceAccuracy, OccurrenceRow, Participation,
};
use engine_api::{
    addresses_match, to_local, AccountId, CalendarDateTime, Event, OverrideSurvival, Provider,
    ProviderKey, TimeZoneId, UtcDateTime,
};

use crate::{calendar::CalendarInputs, App};

/// Whether a **series** edit on a master carrying `has_overrides` must warn:
/// the two facts ANDed, never either alone, because each alone is noise. The
/// server's policy says what a series edit costs (a transport that keeps
/// every override costs nothing), and whether this series holds any
/// per-occurrence change says whether the user has anything to lose; a
/// warning on a clean series is what teaches people to click past the one
/// that mattered. The rebuild marks each occurrence row with it, and the
/// write path consults the same shape, so the row a surface asks with and the
/// write it then sends can never disagree.
pub(crate) fn series_edit_warns(survival: Option<OverrideSurvival>, has_overrides: bool) -> bool {
    has_overrides && survival.is_some_and(OverrideSurvival::warns_on_series_edit)
}

/// How this account stands in a stored event, for the row's `participation`.
///
/// Four cases, and collapsing any two of them is a bug a grid can draw: an
/// event with no participants is the user's own appointment (a commitment, or
/// it lands in the unanswered bucket the hold marking reads); participants but
/// none of them us is a commitment for the same reason; a meeting we called is
/// a commitment by definition (RFC 5546 §3.2.1 has the organiser attending, so
/// a server that writes the organiser in with no `PARTSTAT` is not saying "we
/// do not know"); and an event we were invited to reads our own line.
pub(crate) fn diary_participation(event: &Event, my_address: &str) -> Participation {
    if event.participants.is_empty() || matched_address(event, my_address).is_none() {
        return Participation::Accepted;
    }
    let answer = participation_of(event, my_address);
    if answer == Participation::NeedsAction && organized_by_us(event, my_address) {
        return Participation::Accepted;
    }
    answer
}

/// Whether this event is **ours to reshape**: the gate on dragging one about
/// on the grid, strictly narrower than the calendar's write right.
///
/// Two cases, the two halves of "your own diary": nobody was invited (a
/// private appointment; moving it tells no one), or we called the meeting (an
/// organiser moving their own meeting is the normal way a meeting gets moved,
/// and the server's scheduling layer sends the updates). Everything else is
/// someone else's, and the right affordance there is a proposal, which does
/// not exist yet.
pub(crate) fn owns_or_organizes(event: &Event, my_address: &str) -> bool {
    event.participants.is_empty() || organized_by_us(event, my_address)
}

/// Whether **we** called this meeting: an organiser (`owner` role) at one of
/// our own addresses.
///
/// Written over every owner participant rather than the one attendee the
/// address matched, because the two ways a server represents "the organiser is
/// also attending" are both in the wild (one merged participant, two lines),
/// and the address is the same in both.
fn organized_by_us(event: &Event, my_address: &str) -> bool {
    event
        .participants
        .iter()
        .filter(|participant| participant.has_role(&engine_api::ParticipantRole::Owner))
        .filter_map(|participant| participant.email.as_deref())
        .any(|organizer| addresses_match(organizer, my_address))
}

/// The first participant address that is this account's own, verbatim: the
/// line an RSVP must name to be the account's answer rather than a second
/// attendee's.
pub(crate) fn matched_address(event: &Event, my_address: &str) -> Option<String> {
    event.participants.iter().find_map(|participant| {
        let attendee = participant.email.as_deref()?;
        addresses_match(attendee, my_address).then(|| attendee.to_owned())
    })
}

/// This account's own `PARTSTAT` on the event, or the unanswered hold when the
/// line carries none (RFC 5545's default) or no line is ours.
fn participation_of(event: &Event, my_address: &str) -> Participation {
    event
        .participants
        .iter()
        .find(|participant| {
            participant
                .email
                .as_deref()
                .is_some_and(|attendee| addresses_match(attendee, my_address))
        })
        .map_or(Participation::NeedsAction, |participant| match participant
            .participation_status
            .as_str()
        {
            "accepted" => Participation::Accepted,
            "declined" => Participation::Declined,
            "tentative" => Participation::Tentative,
            "delegated" => Participation::Delegated,
            // An unknown or vendor status is the honest "we do not know that
            // you answered", and the safe one: the hold stays a hold.
            _ => Participation::NeedsAction,
        })
}

/// How an occurrence row's instants were resolved, from the master's time
/// form: the promise a grid may make about the times it draws.
pub(crate) fn accuracy_of(start: &CalendarDateTime) -> OccurrenceAccuracy {
    match start {
        CalendarDateTime::Date(_) => OccurrenceAccuracy::AllDay,
        CalendarDateTime::Floating(_) => OccurrenceAccuracy::Floating,
        CalendarDateTime::Zoned { .. } => OccurrenceAccuracy::Zoned,
    }
}

/// An occurrence's **original** start as a wall clock in the event's own
/// terms: the token a drag or a one-occurrence write hands back, minted from
/// the row's identity slot (`recurrence_id` when the instance was overridden,
/// its own start when it was not).
///
/// Opaque to the host: an identifier for "the Tuesday you dragged", never a
/// time to compute with. The core mints it and [`crate::calendar::edit`]
/// renders it back. A floating event resolves through the display zone, an
/// all-day one keeps the instant's own date (localising a stored midnight
/// would drag it onto a neighbouring day), and a zoned one uses its own zone.
pub(crate) fn occurrence_token(
    event: &Event,
    instant: UtcDateTime,
    display_zone: &TimeZoneId,
) -> Option<engine_api::LocalDateTime> {
    match &event.start {
        CalendarDateTime::Date(_) => {
            engine_api::LocalDateTime::new(instant.year(), instant.month(), instant.day(), 0, 0, 0)
                .ok()
        }
        CalendarDateTime::Floating(_) => to_local(instant, display_zone).ok(),
        CalendarDateTime::Zoned { zone, .. } => to_local(instant, zone).ok(),
    }
}

impl<P: Provider> App<P> {
    /// Rebuilds the calendar snapshot from the **already-synced** store and
    /// publishes it on [`Surface::Calendar`](crate::Surface::Calendar): every
    /// account's calendars, the visible window's joined occurrences, the
    /// write states, and the now line. A store failure per account is logged
    /// and that account contributes nothing, because a snapshot half-built
    /// from a failed read is a diary that lies about being empty.
    pub(crate) async fn rebuild_calendar(&self) {
        let inputs = self.calendar_inputs();
        let mut calendars = Vec::new();
        let mut occurrences = Vec::new();
        for handle in self.account_handles().await {
            let survival = handle
                .providers
                .first()
                .and_then(|provider| provider.connection_info().capabilities.override_survival());
            let rows = self
                .calendar_rows(&handle.id, &handle.identity.email, &inputs, survival)
                .await;
            calendars.extend(rows.0);
            occurrences.extend(rows.1);
        }
        // Each account's rows are sorted; the merge across accounts is not.
        occurrences.sort_by(|a, b| {
            (a.start, a.end, a.account.as_str(), a.event.as_str()).cmp(&(
                b.start,
                b.end,
                b.account.as_str(),
                b.event.as_str(),
            ))
        });
        self.calendar.publish(CalendarSnapshot {
            calendars,
            occurrences,
            write_states: inputs.writes,
            reply_prompts: inputs.prompts,
            now_data: Some(NowLineData {
                now: crate::calendar::now_utc().unwrap_or(inputs.window.start()),
                zone: inputs.zone.as_str().to_owned(),
            }),
        });
    }

    /// One account's calendar rows and joined occurrence rows over the window.
    /// The masters are resolved in one targeted read
    /// ([`Engine::events_by_keys`]) over exactly the keys the window's rows
    /// name, never a decode of the account's whole diary. `survival` is the
    /// account transport's series-edit policy, the fact the series warning's
    /// second half reads.
    async fn calendar_rows(
        &self,
        account: &AccountId,
        my_address: &str,
        inputs: &CalendarInputs,
        survival: Option<OverrideSurvival>,
    ) -> (Vec<CalendarRow>, Vec<OccurrenceRow>) {
        let Ok(engine_calendars) = self.engine.calendars(account).await else {
            return (Vec::new(), Vec::new());
        };
        let calendars = engine_calendars
            .iter()
            .map(|calendar| CalendarRow {
                account: account.clone(),
                id: calendar.id.clone(),
                name: calendar.name.clone(),
                color: calendar.color.clone(),
                visible: inputs.is_visible(account, &calendar.id),
                can_write: calendar.access.may_write,
                is_default: calendar.is_default,
            })
            .collect::<Vec<_>>();
        let write_rights: HashMap<String, bool> = engine_calendars
            .iter()
            .map(|calendar| {
                (
                    calendar.id.key().as_str().to_owned(),
                    calendar.access.may_write,
                )
            })
            .collect();
        let Ok(rows) = self.engine.occurrences_in(account, inputs.window).await else {
            return (calendars, Vec::new());
        };
        let keys: Vec<ProviderKey> = rows
            .iter()
            .map(|row| row.event.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let Ok(masters) = self.engine.events_by_keys(account, &keys).await else {
            return (calendars, Vec::new());
        };
        let masters: HashMap<String, Event> = masters
            .into_iter()
            .map(|event| (event.id.key().as_str().to_owned(), event))
            .collect();
        let joined = rows
            .into_iter()
            .filter_map(|row| {
                let master = masters.get(row.event.as_str())?;
                // A tombstone the store still lists, and a meeting this
                // account declined, render nowhere: the first is gone, the
                // second the user said no to.
                if master.is_cancelled()
                    || diary_participation(master, my_address) == Participation::Declined
                {
                    return None;
                }
                let calendar = master.calendars.iter().next()?.clone();
                let participation = diary_participation(master, my_address);
                let can_write = write_rights
                    .get(calendar.key().as_str())
                    .copied()
                    .unwrap_or(false);
                if !inputs.is_visible(account, &calendar) {
                    return None;
                }
                Some(OccurrenceRow {
                    account: account.clone(),
                    event: row.event,
                    calendar,
                    title: master.title.clone(),
                    start: row.start,
                    end: row.end,
                    all_day: master.is_all_day(),
                    recurring: master.is_recurring(),
                    overridden: row.recurrence_id.is_some(),
                    occurrence_start: master
                        .is_recurring()
                        .then(|| {
                            occurrence_token(
                                master,
                                row.recurrence_id.unwrap_or(row.start),
                                &inputs.zone,
                            )
                        })
                        .flatten(),
                    accuracy: accuracy_of(&master.start),
                    participation,
                    can_move: can_write && owns_or_organizes(master, my_address),
                    series_edit_warns: series_edit_warns(
                        survival,
                        master
                            .recurrence
                            .as_ref()
                            .is_some_and(|recurrence| !recurrence.overrides.is_empty()),
                    ),
                })
            })
            .collect();
        (calendars, joined)
    }
}

#[cfg(test)]
#[path = "../calendar_snapshot_tests.rs"]
mod tests;
