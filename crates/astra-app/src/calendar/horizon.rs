// SPDX-License-Identifier: GPL-3.0-only
//! The calendar's window and horizon machinery, and its reading of the
//! persisted preferences that shape both: where the visible window sits
//! before the first `SetRange`, how wide the cold window is, the watermark
//! that widens a store's expansion to cover the window, the re-expansion a
//! zone change owes, and the zone and week start every rebuild and rule
//! read.
//!
//! The horizon is the engine's ([`Engine::expand_horizon`] materializes a
//! window's occurrences once, so a window nothing has expanded renders
//! confidently empty); this module owns when the app widens it and what the
//! persisted preferences feed into the spans it asks for. The cold window's
//! days and the display zone come from the preferences document, so a
//! setting the user changes moves the calendar with no restart and no code
//! path of its own: the reads here are the whole consumption.

use engine_api::{AccountId, Horizon, Provider, TimeZoneId, UtcDateTime};

use crate::calendar::now_utc;
use crate::App;

/// How far ahead of the window the horizon widens when it must, in days: the
/// coarse chunk of the survey's contract. Expansion re-derives every stored
/// event, so it moves in month-sized steps against the watermark rather than
/// once per page.
const HORIZON_CHUNK_DAYS: u64 = 31;

/// The visible window the grids lay out over.
#[derive(Clone, Copy)]
pub(crate) struct Window {
    /// The UTC instant the window opens at.
    pub(crate) start: UtcDateTime,
    /// How many days the window spans.
    pub(crate) days: u16,
}

impl Window {
    /// The half-open UTC span the window covers.
    pub(crate) fn horizon(self) -> Horizon {
        let span = core::time::Duration::from_secs(u64::from(self.days) * 86_400);
        Horizon::new(
            self.start,
            self.start.checked_add(span).unwrap_or(self.start),
        )
        .unwrap_or_else(|_| {
            // A horizon refuses an empty range; a zero-day window is the only
            // way to ask for one, and clamping to one second keeps every read
            // valid without inventing days the host never asked to see.
            Horizon::new(
                self.start,
                self.start
                    .checked_add(core::time::Duration::from_secs(1))
                    .expect("one second is representable"),
            )
            .expect("a one-second window is never empty")
        })
    }
}

/// The window a rebuild or a refresh falls back to before the first
/// `SetRange`: `days` around `now`, a week of them behind, so a cold app
/// still paints the week it is in. The days come from the persisted horizon
/// preference; one definition, because a refresh that synced a different span
/// than the one the rebuild reads would materialize occurrences the grid
/// never shows, and show ones it never synced.
pub(crate) fn default_window(now: UtcDateTime, days: u16) -> Window {
    Window {
        start: now
            .checked_sub(core::time::Duration::from_hours(7 * 24))
            .unwrap_or(now),
        days,
    }
}

impl<P: Provider> App<P> {
    /// Widens the store's expansion horizon to cover the current window when
    /// it has escaped the watermark, in one coarse chunk per widening.
    pub(crate) async fn ensure_horizon(&self) {
        let window = self.calendar_inputs().window;
        self.widen_horizon_to(window).await;
    }

    /// Widens every account's expansion horizon to cover `span` when it has
    /// escaped the watermark, in one coarse chunk per widening, and reports
    /// whether every store now covers it.
    ///
    /// The watermark is shared across accounts, so one widening expands every
    /// account's store: a span the watermark claims but one store never
    /// expanded to would make that account's later window inside it skip its
    /// own expansion and render confidently empty. The watermark rolls back
    /// to whatever stood before (nothing included) when any account's
    /// expansion refuses (a concurrent sync holds the scope), which is the
    /// exact failure the watermark exists to prevent: re-syncing cannot
    /// repair it, for a delta with no changes derives no occurrences. The
    /// stores an expansion already covered stay covered; an occurrence read
    /// is windowed, so a store wider than the watermark costs nothing.
    pub(crate) async fn widen_horizon_to(&self, span: Horizon) -> bool {
        let (want, previous) = {
            let mut state = self.calendar_state.lock().expect("calendar mutex poisoned");
            let end = span
                .end()
                .checked_add(core::time::Duration::from_secs(HORIZON_CHUNK_DAYS * 86_400))
                .unwrap_or_else(|| span.end());
            let widened = match state.horizon {
                Some((low, high)) if low <= span.start() && end <= high => return true,
                Some((low, high)) => (low.min(span.start()), high.max(end)),
                None => (span.start(), end),
            };
            (widened, state.horizon.replace(widened))
        };
        let horizon = Horizon::new(want.0, want.1).expect("a widened horizon is never empty");
        let zone = self.calendar_inputs().zone;
        let mut widened = true;
        for handle in self.account_handles().await {
            if let Err(err) = self.engine.expand_horizon(&handle.id, horizon, &zone).await {
                widened = false;
                log::warn!("calendar: the horizon could not be widened for one account: {err}");
            }
        }
        if !widened {
            self.calendar_state
                .lock()
                .expect("calendar mutex poisoned")
                .horizon = previous;
        }
        widened
    }

    /// Re-expands every account's stored events over the full span the app
    /// has expanded under the standing display zone, answering the accounts
    /// whose re-expansion refused or failed: the recovery a zone change
    /// performs, because a floating event's stored instant is only correct
    /// for the zone it was expanded under and a zone change without a
    /// re-expansion silently shifts every floating event by the offset
    /// between the two.
    ///
    /// The span is the watermark (or the default window when none stands),
    /// the same span a refresh syncs, never the current window alone: the
    /// engine's expansion removes every event's occurrence rows and
    /// rewrites only the span asked for, so a re-expansion over the window
    /// would strand every other span the watermark covers while the
    /// watermark still claimed it, and a later range inside it skips its
    /// own widening and renders confidently empty. Expansion re-derives
    /// every stored event on every call, which is exactly what the change
    /// wants and why nothing else calls it this eagerly.
    pub(crate) async fn reexpand_calendar(&self) -> Vec<AccountId> {
        let span = self.sync_horizon();
        let zone = self.display_zone();
        let mut stranded = Vec::new();
        for handle in self.account_handles().await {
            if let Err(err) = self.engine.expand_horizon(&handle.id, span, &zone).await {
                let at = self.account_ordinal(&handle.id).await;
                log::warn!("calendar[a{at}]: the zone change could not re-expand: {err}");
                stranded.push(handle.id.clone());
            }
        }
        stranded
    }

    /// The horizon a refresh's sync covers: the full watermark when one stands
    /// (so a refresh never narrows what the app has expanded), the default
    /// window around now otherwise, matching the window a cold rebuild reads.
    pub(crate) fn sync_horizon(&self) -> Horizon {
        let state = self.calendar_state.lock().expect("calendar mutex poisoned");
        match state.horizon {
            Some((low, high)) => Horizon::new(low, high).unwrap_or_else(|_| {
                Window {
                    start: low,
                    days: 1,
                }
                .horizon()
            }),
            None => default_window(
                now_utc().unwrap_or_else(|| {
                    UtcDateTime::new(2026, 1, 1, 0, 0, 0).expect("a representable instant")
                }),
                self.horizon_days(),
            )
            .horizon(),
        }
    }

    /// The calendar's display zone, from the persisted preference: the id the
    /// engine's bundled tzdata resolves, or UTC for one it cannot (a
    /// hand-edited preferences file), so every read stays valid whatever the
    /// file holds. The set arm refuses an unresolvable id, so the fallback
    /// here is the belt to that brace.
    pub(crate) fn display_zone(&self) -> TimeZoneId {
        let name = self.pane.lock().expect("preferences mutex poisoned").zone();
        TimeZoneId::iana(&name)
            .ok()
            .filter(engine_api::is_supported_zone)
            .unwrap_or_else(TimeZoneId::utc)
    }

    /// The cold window's length in days, from the persisted horizon
    /// preference, clamped into the bounds the set arm enforces.
    pub(crate) fn horizon_days(&self) -> u16 {
        self.pane
            .lock()
            .expect("preferences mutex poisoned")
            .horizon_days()
    }

    /// The day the week starts on, from the persisted preference, in the
    /// engine's own weekday words: the WKST a created rule that names no
    /// weekday carries.
    pub(crate) fn calendar_week_start(&self) -> engine_api::Weekday {
        super::edit::weekday_of(
            self.pane
                .lock()
                .expect("preferences mutex poisoned")
                .first_day_of_week(),
        )
    }
}
