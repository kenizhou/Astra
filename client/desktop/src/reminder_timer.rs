// SPDX-License-Identifier: GPL-3.0-only
//! The periodic reminder wake timer: the host's standing cadence beside the
//! snooze timer and the delivery loop. One process-lifetime task ticks every
//! minute, reads the reminder store's due entries, and wakes them through
//! the core's wake body, delivering the notices through the notification
//! bridge. The timer is stateless over the store, so a restart re-reads the
//! due entries: a wake due while the app was closed fires at the next launch,
//! not the exact instant (the recorded closed-app gap).
//!
//! The interval and the tick limit are debug-overridable for deterministic
//! tests and harness runs; a release build holds the standing cadence and
//! never stops.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;

use astra_app::NewMailBatch;

use crate::{notify::NotifyBridge, AppPort, AppRuntime};

/// The standing reminder wake cadence: one minute.
pub(crate) const REMINDER_TICK: Duration = Duration::from_secs(60);

/// The tick interval: the standing cadence, or the debug override.
pub(crate) fn reminder_interval() -> Duration {
    #[cfg(debug_assertions)]
    if let Ok(raw) = std::env::var("ASTRA_REMINDER_SECONDS") {
        if let Ok(seconds) = raw.parse::<u64>() {
            return Duration::from_secs(seconds.max(1));
        }
    }
    REMINDER_TICK
}

/// How many ticks the timer runs before stopping: the deterministic bound a
/// test or harness run names, `None` for the process lifetime.
pub(crate) fn reminder_limit() -> Option<u64> {
    #[cfg(debug_assertions)]
    {
        parse_reminder_limit(std::env::var("ASTRA_REMINDER_LIMIT").ok().as_deref())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The limit override as a positive tick count, anything else ignored.
#[cfg(debug_assertions)]
fn parse_reminder_limit(raw: Option<&str>) -> Option<u64> {
    raw.and_then(|value| value.parse::<u64>().ok())
        .filter(|limit| *limit > 0)
}

/// The ports one tick drives: the seams the state machine is tested over, so
/// a fake clock and a scripted due gate prove the cadence without a server.
#[async_trait]
pub(crate) trait ReminderHost: Send + Sync {
    /// Waits out `delay`, one tick.
    async fn wait(&self, delay: Duration);

    /// The current unix instant the tick wakes at.
    fn now(&self) -> i64;

    /// Whether any reminder entry is due at `now`: the gate a tick's wake is
    /// gated on, read from the store.
    fn due(&self, now: i64) -> bool;

    /// Runs the wake body over the entries due at `now`.
    async fn wake(&self, now: i64);
}

/// One reminder wake timer's state machine. The run loop is `loop { step }`;
/// the tests drive steps directly over a fake host.
pub(crate) struct ReminderTimer<H: ReminderHost> {
    host: H,
    /// How many ticks remain before the timer stops, `None` for ever.
    remaining: Option<u64>,
}

impl<H: ReminderHost> ReminderTimer<H> {
    /// A timer over `host`, under the standing limit.
    pub(crate) fn new(host: H) -> Self {
        Self {
            host,
            remaining: reminder_limit(),
        }
    }

    /// A timer over `host` with the limit handed to it: the deterministic
    /// shape the tests drive.
    #[cfg(test)]
    pub(crate) fn with_limit(host: H, remaining: Option<u64>) -> Self {
        Self { host, remaining }
    }

    /// Runs one transition of the timer, answering whether the timer
    /// continues: one tick waits out the interval, wakes only while an entry
    /// is due, and counts the tick against the limit.
    pub(crate) async fn step(&mut self) -> bool {
        self.host.wait(reminder_interval()).await;
        let now = self.host.now();
        if self.host.due(now) {
            self.host.wake(now).await;
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

/// Runs the reminder wake cadence for the life of the process: one timer over
/// the app core, the reminder store, and the notification bridge, spawned
/// beside the snooze timer and holding nothing of the state beyond the three
/// `Arc`s, so the `AppState` drop order stays exactly as it stands.
pub(crate) fn spawn(
    app: Arc<dyn AppPort>,
    store: Arc<astra_reminders::ReminderStore>,
    notify: Arc<NotifyBridge>,
    runtime: &AppRuntime,
) {
    runtime.spawn(async move {
        let mut timer = ReminderTimer::new(HostTimer { app, store, notify });
        while timer.step().await {}
    });
}

/// The production host: the core's wake body, the reminder store the due gate
/// reads, and the bridge the notices cross through.
struct HostTimer {
    /// The app core the wake runs through.
    app: Arc<dyn AppPort>,
    /// The device-local reminder store the due gate reads.
    store: Arc<astra_reminders::ReminderStore>,
    /// The notification bridge the woken notices cross through.
    notify: Arc<NotifyBridge>,
}

#[async_trait]
impl ReminderHost for HostTimer {
    async fn wait(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }

    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
            })
    }

    fn due(&self, now: i64) -> bool {
        self.store.due(now).is_ok_and(|entries| !entries.is_empty())
    }

    async fn wake(&self, now: i64) {
        match self.app.wake_due_reminders(now).await {
            Ok(messages) => {
                let enabled = self.app.settings().settings.notifications_enabled;
                let batch = NewMailBatch {
                    messages,
                    overflow: 0,
                };
                self.notify.deliver_reminders(&batch, enabled);
            }
            Err(err) => log::warn!("reminders: the wake tick failed: {err}"),
        }
    }
}

#[cfg(test)]
#[path = "reminder_timer_tests.rs"]
mod tests;
