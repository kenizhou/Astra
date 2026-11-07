// SPDX-License-Identifier: GPL-3.0-only
//! The periodic calendar refresh timer: the host's standing cadence beside
//! the delivery loop, ported from the reference's runtime timers. One
//! process-lifetime task ticks every five minutes, and a tick reaches the
//! provider only while at least one account stands in the dial table (the
//! same liveness the delivery supervisor reads). The refresh is the core's
//! change-gated body, so a tick that changed nothing signals nothing.
//!
//! The interval and the tick limit are debug-overridable for deterministic
//! tests and harness runs; a release build holds the five-minute cadence
//! and never stops.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;

use crate::{AccountDials, AppPort, AppRuntime};

/// The standing calendar refresh cadence: five minutes.
pub(crate) const CALENDAR_REFRESH_TICK: Duration = Duration::from_secs(300);

/// The tick interval: the five-minute cadence, or the debug override.
pub(crate) fn calendar_refresh_interval() -> Duration {
    #[cfg(debug_assertions)]
    if let Ok(raw) = std::env::var("ASTRA_CALENDAR_REFRESH_SECONDS") {
        if let Ok(seconds) = raw.parse::<u64>() {
            return Duration::from_secs(seconds.max(1));
        }
    }
    CALENDAR_REFRESH_TICK
}

/// How many ticks the timer runs before stopping: the deterministic bound a
/// test or harness run names, `None` for the process lifetime.
pub(crate) fn calendar_refresh_limit() -> Option<u64> {
    #[cfg(debug_assertions)]
    {
        parse_calendar_refresh_limit(
            std::env::var("ASTRA_CALENDAR_REFRESH_LIMIT")
                .ok()
                .as_deref(),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The limit override as a positive tick count, anything else ignored.
#[cfg(debug_assertions)]
fn parse_calendar_refresh_limit(raw: Option<&str>) -> Option<u64> {
    raw.and_then(|value| value.parse::<u64>().ok())
        .filter(|limit| *limit > 0)
}

/// The ports one tick drives: the seams the state machine is tested over,
/// so a fake clock and a scripted gate prove the cadence without a server.
#[async_trait]
pub(crate) trait TimerHost: Send + Sync {
    /// Waits out `delay`, one tick.
    async fn wait(&self, delay: Duration);

    /// Whether at least one account stands connected: the gate a tick's
    /// refresh is gated on.
    fn connected(&self) -> bool;

    /// Runs one refresh through the change-gated body.
    async fn refresh(&self);
}

/// One calendar refresh timer's state machine. The run loop is
/// `loop { step }`; the tests drive steps directly over a fake host.
pub(crate) struct CalendarTimer<H: TimerHost> {
    host: H,
    /// How many ticks remain before the timer stops, `None` for ever.
    remaining: Option<u64>,
}

impl<H: TimerHost> CalendarTimer<H> {
    /// A timer over `host`, under the standing limit.
    pub(crate) fn new(host: H) -> Self {
        Self {
            host,
            remaining: calendar_refresh_limit(),
        }
    }

    /// A timer over `host` with the limit handed to it: the deterministic
    /// shape the tests drive.
    #[cfg(test)]
    pub(crate) fn with_limit(host: H, remaining: Option<u64>) -> Self {
        Self { host, remaining }
    }

    /// Runs one transition of the timer, answering whether the timer
    /// continues: one tick waits out the interval, refreshes only while an
    /// account stands connected, and counts the tick against the limit.
    pub(crate) async fn step(&mut self) -> bool {
        self.host.wait(calendar_refresh_interval()).await;
        if self.host.connected() {
            self.host.refresh().await;
        }
        if let Some(remaining) = &mut self.remaining {
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                return false;
            }
        }
        true
    }
}

/// Runs the calendar refresh cadence for the life of the process: one timer
/// over the app core and the dial table, spawned beside the delivery loop
/// and holding nothing of the state beyond the two `Arc`s, so the
/// `AppState` drop order stays exactly as it stands.
pub(crate) fn spawn(app: Arc<dyn AppPort>, dials: Arc<AccountDials>, runtime: &AppRuntime) {
    runtime.spawn(async move {
        let mut timer = CalendarTimer::new(HostTimer { app, dials });
        while timer.step().await {}
    });
}

/// The production host: the app core's change-gated refresh body and the
/// dial table the connected gate reads, the same liveness the delivery
/// supervisor's adoption loop reads.
struct HostTimer {
    /// The app core the refresh runs through.
    app: Arc<dyn AppPort>,
    /// The registered accounts, whose presence is the connected gate.
    dials: Arc<AccountDials>,
}

#[async_trait]
impl TimerHost for HostTimer {
    async fn wait(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }

    fn connected(&self) -> bool {
        !self.dials.ids().is_empty()
    }

    async fn refresh(&self) {
        self.app.refresh_calendars_if_changed().await;
    }
}

#[cfg(test)]
#[path = "calendar_timer_tests.rs"]
mod tests;
