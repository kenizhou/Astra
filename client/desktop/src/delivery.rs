// SPDX-License-Identifier: GPL-3.0-only
//! The background delivery driver: one loop per dialled account, the
//! engine's prescribed watch-or-poll cycle. The loop joins the account's
//! first pass, then watches when the account can (a `Changed` runs the
//! account's normal single-account pass; a `KeepAlive` is a heartbeat; an
//! error reconnects with backoff and syncs before re-watching) and polls
//! the account's own cadence when it cannot. Every pass it lands drains
//! the new-mail queue through the notification bridge, and every outcome
//! of its own folds into the delivery state the sync-progress snapshot
//! carries.

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use async_trait::async_trait;

use astra_account::WatchEvent;
use astra_app::{NewMailBatch, PassOutcome, DEFAULT_POLL_MINUTES};
use engine_api::AccountId;

use crate::{notify::NotifyBridge, AccountDials, AppPort, AppRuntime};

/// Where the reconnect backoff starts.
pub(crate) const RECONNECT_BASE: Duration = Duration::from_secs(1);

/// How often the supervisor looks for accounts to adopt: the window within
/// which an account added at runtime gains its driver.
pub(crate) const ADOPT_TICK: Duration = Duration::from_secs(10);

/// What one open answered: a standing watch, the poll cadence, a failure
/// the loop records, or the account leaving the dial table.
pub(crate) enum OpenWatch {
    /// A standing watch session.
    Watch(Box<dyn astra_account::Watch>),
    /// The account has no push face; its cadence carries it.
    Poll,
    /// The open itself failed, carrying the outcome the loop records.
    Failed(PassOutcome),
    /// The account is no longer registered; the loop stops.
    Gone,
}

/// The ports one account's loop drives: the seams the state machine is
/// tested over, so a fake clock and scripted watches prove the cycle
/// without a server.
#[async_trait]
pub(crate) trait DeliveryHost: Send + Sync {
    /// Opens the account's watch face, or the answer the dial table gave.
    async fn open(&self) -> OpenWatch;

    /// Runs one single-account pass and answers the new mail it landed.
    async fn pass(&self) -> NewMailBatch;

    /// Records an outcome of the loop's own, beside the ones the pass
    /// paths record for it.
    fn record(&self, outcome: PassOutcome);

    /// Waits out `delay`, a cadence or a backoff.
    async fn wait(&self, delay: Duration);

    /// The account's poll cadence, read fresh each cycle.
    fn cadence(&self) -> Duration;

    /// Delivers a landed batch under the notification rules.
    fn notify(&self, batch: &NewMailBatch);
}

/// The exponential reconnect backoff: doubling from
/// [`RECONNECT_BASE`](self::RECONNECT_BASE), capped at the account's own
/// cadence, reset by a watch that stood.
#[derive(Default)]
pub(crate) struct Backoff {
    /// The delay the next failure waits, `None` while healthy.
    delay: Option<Duration>,
}

impl Backoff {
    /// The next delay, doubling towards `cap`.
    pub(crate) fn next(&mut self, cap: Duration) -> Duration {
        let delay = self.delay.unwrap_or(RECONNECT_BASE).min(cap);
        self.delay = Some((delay * 2).min(cap));
        delay
    }

    /// Resets to the base: a watch that stood is progress.
    pub(crate) fn reset(&mut self) {
        self.delay = None;
    }
}

/// One account's delivery loop state machine. The run loop is `loop {
/// step }`; the tests drive steps directly over a fake host.
pub(crate) struct Driver<H: DeliveryHost> {
    host: H,
    backoff: Backoff,
    /// The standing watch, when the account has one.
    watching: Option<Box<dyn astra_account::Watch>>,
}

impl<H: DeliveryHost> Driver<H> {
    /// A driver over `host`, nothing standing yet.
    pub(crate) fn new(host: H) -> Self {
        Self {
            host,
            backoff: Backoff::default(),
            watching: None,
        }
    }

    /// Runs one transition of the loop, answering whether the loop
    /// continues.
    ///
    /// The standing watch owns its arm: a `Changed` runs the pass and
    /// delivers what landed, a `KeepAlive` costs nothing, and an error
    /// records its outcome, backs off, and syncs before the next open
    /// re-watches (the engine's prescribed loop). Without a watch, the
    /// open decides: a watch stands, a poll waits the cadence and passes,
    /// a failure records and backs off the same way, and an account that
    /// left the dial table stops the loop.
    pub(crate) async fn step(&mut self) -> bool {
        if let Some(watch) = self.watching.as_mut() {
            match watch.next().await {
                Ok(WatchEvent::Changed) => {
                    let batch = self.host.pass().await;
                    self.host.notify(&batch);
                }
                // A heartbeat, and any quieter word a later engine adds:
                // nothing to run, nothing to wait.
                Ok(_) => {}
                Err(err) => {
                    if let Some(outcome) = watch_failure(&err) {
                        self.host.record(outcome);
                    }
                    self.watching = None;
                    let delay = self.backoff.next(self.host.cadence());
                    self.host.wait(delay).await;
                    // Every reconnect syncs before re-watching: the watch
                    // gap closes with the pass, not the re-dial.
                    let batch = self.host.pass().await;
                    self.host.notify(&batch);
                }
            }
            return true;
        }
        match self.host.open().await {
            OpenWatch::Watch(watch) => {
                self.backoff.reset();
                self.watching = Some(watch);
            }
            OpenWatch::Poll => {
                let cadence = self.host.cadence();
                self.host.wait(cadence).await;
                let batch = self.host.pass().await;
                self.host.notify(&batch);
            }
            OpenWatch::Failed(outcome) => {
                self.host.record(outcome);
                let delay = self.backoff.next(self.host.cadence());
                self.host.wait(delay).await;
                let batch = self.host.pass().await;
                self.host.notify(&batch);
            }
            OpenWatch::Gone => return false,
        }
        true
    }
}

/// The outcome a lost watch records, or `None` when it records nothing:
/// the engine classifies a transport drop retryable, and those are the
/// outage streak's own failures; a permanent watch error is never an
/// outage (D5), so it only drops the session.
fn watch_failure(err: &engine_provider::ProviderError) -> Option<PassOutcome> {
    match err.class() {
        engine_api::FailureClass::Retryable | engine_api::FailureClass::RateLimited => {
            Some(PassOutcome::NetworkFailure)
        }
        _ => None,
    }
}

/// Runs the delivery: one driver per dialled account, adopted as the dial
/// table gains them and left behind as it drops them. The supervisor runs
/// for the life of the process; each driver joins its account's first pass
/// before its first watch, so the boot pass is the sync the engine's
/// prescribed loop rests on and no pass is duplicated.
pub(crate) fn spawn(
    app: Arc<dyn AppPort>,
    dials: Arc<AccountDials>,
    notify: Arc<NotifyBridge>,
    runtime: &AppRuntime,
) {
    runtime.spawn(async move {
        let mut adopted: BTreeSet<String> = BTreeSet::new();
        loop {
            let ids = dials.ids();
            // An account that left the table may come back (a re-add); its
            // driver stopped itself, so the id is adoptable again.
            adopted.retain(|id| ids.contains(id));
            for id in ids {
                if !adopted.insert(id.clone()) {
                    continue;
                }
                let Some(account) = AccountId::try_from(id.as_str()).ok() else {
                    log::warn!(
                        "delivery: an account id could not be read; it rides manual refresh"
                    );
                    continue;
                };
                let host = Arc::new(HostDelivery {
                    app: Arc::clone(&app),
                    dials: Arc::clone(&dials),
                    notify: Arc::clone(&notify),
                    account,
                });
                tokio::spawn(async move {
                    host.app.wait_first_pass(&host.account).await;
                    let mut driver = Driver::new(host);
                    while driver.step().await {}
                });
            }
            tokio::time::sleep(ADOPT_TICK).await;
        }
    });
}

/// The production host: the app core for the pass and the drain, the dial
/// table for the watch, the notification bridge for what landed.
struct HostDelivery {
    /// The app core the pass runs through and the queue drains from.
    app: Arc<dyn AppPort>,
    /// The registered accounts, where the watch's connections live.
    dials: Arc<AccountDials>,
    /// The notification bridge.
    notify: Arc<NotifyBridge>,
    /// The account this loop delivers.
    account: AccountId,
}

#[async_trait]
impl DeliveryHost for Arc<HostDelivery> {
    async fn open(&self) -> OpenWatch {
        let Some((config, connections)) = self.dials.of(self.account.as_str()) else {
            return OpenWatch::Gone;
        };
        match astra_account::open_watch(&connections, &config).await {
            Ok(astra_account::WatchFace::Watch(watch)) => OpenWatch::Watch(watch),
            Ok(astra_account::WatchFace::Poll) => OpenWatch::Poll,
            Err(astra_account::AccountError::SigninRejected(detail)) => {
                log::warn!("delivery: an account's watch was refused its credential; it waits to be signed in again: {detail}");
                OpenWatch::Failed(PassOutcome::PermanentFailure)
            }
            Err(err) => {
                log::info!("delivery: a watch could not be opened; reconnecting: {err}");
                OpenWatch::Failed(PassOutcome::NetworkFailure)
            }
        }
    }

    async fn pass(&self) -> NewMailBatch {
        self.app.sync_added_account(&self.account).await;
        self.app.drain_new_mail()
    }

    fn record(&self, outcome: PassOutcome) {
        self.app.record_pass_outcome(&self.account, outcome);
    }

    async fn wait(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }

    fn cadence(&self) -> Duration {
        let minutes = self
            .app
            .settings()
            .accounts
            .iter()
            .find(|row| row.account == self.account)
            .and_then(|row| row.sync)
            .map_or(DEFAULT_POLL_MINUTES, |sync| sync.poll_minutes);
        Duration::from_secs(u64::from(minutes) * 60)
    }

    fn notify(&self, batch: &NewMailBatch) {
        let enabled = self.app.settings().settings.notifications_enabled;
        self.notify.deliver(batch, enabled);
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
