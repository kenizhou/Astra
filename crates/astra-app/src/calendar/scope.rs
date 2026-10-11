// SPDX-License-Identifier: GPL-3.0-only
//! Which occurrence a calendar write is allowed to name. Ported from
//! mail-calendar's `calendar_scope`: the one-vs-all question is the
//! surface's to ask (the intent carries `Option<LocalDateTime>`), and this
//! module is the other half, checking the **answer** before anything is
//! sent.
//!
//! The token a surface hands back is one this core minted for a block it
//! drew, so the check is not "is this a plausible time" but "is this an
//! occurrence we have", asked against the store the grid was drawn from and
//! answered by re-minting rather than by parsing, so it cannot drift from
//! the emitter. It lives beside the write tail rather than in the mapping
//! layer because only the store knows which occurrences a series actually
//! has: a builder can see that an event has a rule; it cannot see that the
//! rule produces the Tuesday it is being asked about.
//!
//! What it catches is not a misbehaving host so much as a **wrong token**:
//! one minted from the wrong field, one a host built itself instead of
//! handing ours back, or one that was valid until the series changed
//! underneath it. Any of those addresses no instance, and the transports do
//! not agree about what happens next: a second override split at a time the
//! rule never produces, or a silent no-op reported as a save.

use engine_api::{AccountId, Horizon, LocalDateTime, Provider, UtcDateTime};

use crate::calendar::{
    drag::{date_at, from_civil},
    edit::CalendarEditError,
    snapshot::occurrence_token,
};
use crate::App;

impl<P: Provider> App<P> {
    /// The occurrence-token gate every occurrence-scoped write passes: the
    /// token is re-minted from the store and compared, never parsed back, so
    /// the check cannot drift from the emitter. A mismatch is the
    /// stale-token refusal ([`CalendarEditError::StaleOccurrence`]): a token
    /// that went stale when the series changed under the view that minted
    /// it, or one no view of this store ever minted at all. A `None`
    /// occurrence is a series write and passes by definition.
    pub(crate) async fn check_occurrence_token(
        &self,
        account: &AccountId,
        base: &engine_api::Event,
        occurrence: Option<LocalDateTime>,
    ) -> Result<(), CalendarEditError> {
        let Some(occurrence) = occurrence else {
            return Ok(());
        };
        if self
            .names_a_stored_occurrence(account, base, occurrence)
            .await
        {
            Ok(())
        } else {
            Err(CalendarEditError::StaleOccurrence)
        }
    }

    /// Whether `occurrence` names an occurrence of `base` that this core
    /// actually minted a token for: the store is read over the civil day the
    /// token names, a day either side so the occurrence's own zone cannot put
    /// it outside the window, and any of the event's rows whose re-minted
    /// token equals the one offered is the occurrence named.
    ///
    /// A store read the grid has already done, repeated once per write: not
    /// per frame. A one-off event is named by no token at all, so it answers
    /// `false` for every one, which is also the refusal for "split an
    /// override out of an event that has no series".
    async fn names_a_stored_occurrence(
        &self,
        account: &AccountId,
        base: &engine_api::Event,
        occurrence: LocalDateTime,
    ) -> bool {
        if base.recurrence.is_none() {
            return false;
        }
        let Some(window) = day_window(occurrence) else {
            return false;
        };
        let zone = self.calendar_inputs().zone;
        let rows = self
            .engine
            .occurrences_in(account, window)
            .await
            .unwrap_or_default();
        let key = base.id.key();
        rows.iter()
            .filter(|row| row.event.as_str() == key.as_str())
            .any(|row| {
                occurrence_token(base, row.recurrence_id.unwrap_or(row.start), &zone)
                    .is_some_and(|token| token == occurrence)
            })
    }
}

/// The UTC window covering the civil day `occurrence` names, plus a day each
/// side. The token is a wall clock in the event's **own** zone and the store
/// keys occurrences by instants, so a day of slack each way covers every
/// offset a zone can have (±14h) without having to resolve the clock first,
/// and resolving it is exactly what an all-day or floating occurrence cannot
/// do.
fn day_window(occurrence: LocalDateTime) -> Option<Horizon> {
    let day = from_civil(occurrence.year(), occurrence.month(), occurrence.day());
    Horizon::new(midnight_at(day - 1)?, midnight_at(day + 2)?).ok()
}

/// UTC midnight on civil day number `day`.
fn midnight_at(day: i64) -> Option<UtcDateTime> {
    let date = date_at(day);
    UtcDateTime::new(date.year(), date.month(), date.day(), 0, 0, 0).ok()
}

#[cfg(test)]
#[path = "../calendar_gate_tests.rs"]
mod gate_tests;
