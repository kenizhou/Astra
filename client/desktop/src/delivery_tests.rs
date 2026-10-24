// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the delivery loop's state machine over a fake host: a
//! Changed runs the pass and notifies, a KeepAlive costs nothing, a watch
//! error records the outcome and backs off before syncing and re-watching,
//! the poll arm waits the cadence, and the backoff doubles capped at the
//! cadence and resets on a standing watch.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use engine_provider::{ProviderError, ProviderResult};

use astra_account::{Watch, WatchEvent};
use astra_app::{NewMailBatch, NewMailNotice, PassOutcome};
use engine_api::{AccountId, ProviderKey};

use super::{DeliveryHost, Driver, OpenWatch};

/// One recorded action of the fake host, the log the assertions read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// The loop opened its watch face.
    Open,
    /// The loop recorded an outcome of its own.
    Record(PassOutcome),
    /// The loop waited out a delay.
    Wait(Duration),
    /// The loop ran a pass, landing this many messages.
    Pass(usize),
    /// The loop delivered a batch of this many messages.
    Notify(usize),
}

/// A watch whose answers are scripted; an exhausted script parks, so a
/// step that should not read an event simply never finishes.
struct ScriptedWatch {
    steps: Mutex<VecDeque<ProviderResult<WatchEvent>>>,
}

#[async_trait]
impl Watch for ScriptedWatch {
    async fn next(&mut self) -> ProviderResult<WatchEvent> {
        let step = self.steps.lock().unwrap().pop_front();
        match step {
            Some(step) => step,
            None => std::future::pending().await,
        }
    }
}

/// What one scripted open answers.
enum OpenScript {
    Watch,
    /// The poll answer; the queue's default, named for a test that pins
    /// it explicitly.
    #[allow(dead_code)]
    Poll,
    Failed(PassOutcome),
    Gone,
}

/// The fake host: scripted open answers, scripted watch steps, scripted
/// pass batches, and a log of everything the loop did.
struct FakeHost {
    actions: Mutex<Vec<Action>>,
    opens: Mutex<VecDeque<OpenScript>>,
    watch_steps: Mutex<VecDeque<ProviderResult<WatchEvent>>>,
    batches: Mutex<VecDeque<NewMailBatch>>,
    cadence: Duration,
    present: Mutex<bool>,
}

impl FakeHost {
    /// A host whose opens answer `Poll`, its scripts empty.
    fn polling(cadence: Duration) -> Arc<Self> {
        Arc::new(Self {
            actions: Mutex::new(Vec::new()),
            opens: Mutex::new(VecDeque::new()),
            watch_steps: Mutex::new(VecDeque::new()),
            batches: Mutex::new(VecDeque::new()),
            cadence,
            present: Mutex::new(true),
        })
    }

    /// Queues one scripted open answer.
    fn opens_with(self: &Arc<Self>, script: OpenScript) -> &Arc<Self> {
        self.opens.lock().unwrap().push_back(script);
        self
    }

    /// Queues one scripted watch answer, handed to the watch the next open
    /// stands.
    fn watch_yields(self: &Arc<Self>, step: ProviderResult<WatchEvent>) -> &Arc<Self> {
        self.watch_steps.lock().unwrap().push_back(step);
        self
    }

    /// Answers `present` as `standing` from now on.
    fn presence(self: &Arc<Self>, standing: bool) -> &Arc<Self> {
        *self.present.lock().unwrap() = standing;
        self
    }

    /// Queues one scripted pass answer.
    fn passing(self: &Arc<Self>, batch: NewMailBatch) -> &Arc<Self> {
        self.batches.lock().unwrap().push_back(batch);
        self
    }

    fn log(&self, action: Action) {
        self.actions.lock().unwrap().push(action);
    }

    /// The actions so far.
    fn actions(&self) -> Vec<Action> {
        self.actions.lock().unwrap().clone()
    }
}

#[async_trait]
impl DeliveryHost for Arc<FakeHost> {
    async fn open(&self) -> OpenWatch {
        self.log(Action::Open);
        match self.opens.lock().unwrap().pop_front() {
            Some(OpenScript::Watch) => {
                let steps = self.watch_steps.lock().unwrap().drain(..).collect();
                OpenWatch::Watch(Box::new(ScriptedWatch {
                    steps: Mutex::new(steps),
                }))
            }
            Some(OpenScript::Poll) | None => OpenWatch::Poll,
            Some(OpenScript::Failed(outcome)) => OpenWatch::Failed(outcome),
            Some(OpenScript::Gone) => OpenWatch::Gone,
        }
    }

    async fn pass(&self) -> NewMailBatch {
        let landed = self.batches.lock().unwrap().pop_front().unwrap_or_default();
        self.log(Action::Pass(landed.messages.len() + landed.overflow));
        landed
    }

    fn record(&self, outcome: PassOutcome) {
        self.log(Action::Record(outcome));
    }

    async fn wait(&self, delay: Duration) {
        self.log(Action::Wait(delay));
    }

    fn cadence(&self) -> Duration {
        self.cadence
    }

    fn present(&self) -> bool {
        *self.present.lock().unwrap()
    }

    fn notify(&self, batch: &NewMailBatch) {
        self.log(Action::Notify(batch.messages.len() + batch.overflow));
    }
}

/// A batch of `count` messages.
fn landed(count: usize) -> NewMailBatch {
    NewMailBatch {
        messages: (0..count)
            .map(|n| NewMailNotice {
                account: AccountId::try_from("work").unwrap(),
                key: ProviderKey::new(format!("k{n}")).unwrap(),
                from: None,
                subject: Some(format!("Subject {n}")),
                preview: None,
            })
            .collect(),
        overflow: 0,
    }
}

const CADENCE: Duration = Duration::from_mins(30);

#[test]
fn a_changed_runs_the_pass_and_notifies() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Watch)
                .watch_yields(Ok(WatchEvent::Changed))
                .passing(landed(2))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            // The open, then the Changed lands its pass.
            driver.step().await;
            driver.step().await;
            assert_eq!(
                host.actions(),
                vec![Action::Open, Action::Pass(2), Action::Notify(2)],
                "the open stands the watch and each Changed runs the pass"
            );
        });
}

#[test]
fn a_keepalive_costs_nothing() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Watch)
                .watch_yields(Ok(WatchEvent::KeepAlive))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await;
            driver.step().await;
            assert_eq!(
                host.actions(),
                vec![Action::Open],
                "a heartbeat runs no pass and waits no delay"
            );
        });
}

#[test]
fn a_watch_error_records_backs_off_and_syncs_before_re_watching() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Watch)
                .opens_with(OpenScript::Watch)
                .watch_yields(Err(ProviderError::retryable("connection reset")))
                .passing(landed(1))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await; // stand the watch
            driver.step().await; // the error: record, back off, sync
            driver.step().await; // re-open, the sync already behind it
            assert_eq!(
                host.actions(),
                vec![
                    Action::Open,
                    Action::Record(PassOutcome::NetworkFailure),
                    Action::Wait(Duration::from_secs(1)),
                    Action::Pass(1),
                    Action::Notify(1),
                    Action::Open,
                ],
                "every reconnect records, backs off, and syncs before re-watching"
            );
        });
}

#[test]
fn a_permanent_watch_error_records_no_outage() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            // D5: a permanent failure is never an outage; the loop still backs off
            // and syncs, but the delivery state hears nothing from it.
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Watch)
                .watch_yields(Err(ProviderError::permanent("watch refused")))
                .passing(landed(0))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await;
            driver.step().await;
            let actions = host.actions();
            assert!(
                !actions.contains(&Action::Record(PassOutcome::NetworkFailure)),
                "a permanent watch error counts no outage streak: {actions:?}"
            );
            assert_eq!(
                actions[1],
                Action::Wait(Duration::from_secs(1)),
                "it still backs off and syncs"
            );
        });
}

#[test]
fn a_poll_account_waits_the_cadence_then_passes() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(CADENCE).passing(landed(1)).clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await;
            assert_eq!(
                host.actions(),
                vec![
                    Action::Open,
                    Action::Wait(CADENCE),
                    Action::Pass(1),
                    Action::Notify(1),
                ],
                "a poll account rides its own cadence"
            );
        });
}

#[test]
fn a_failed_open_records_backs_off_and_syncs() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Failed(PassOutcome::NetworkFailure))
                .passing(landed(0))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await;
            assert_eq!(
                host.actions(),
                vec![
                    Action::Open,
                    Action::Record(PassOutcome::NetworkFailure),
                    Action::Wait(Duration::from_secs(1)),
                    Action::Pass(0),
                    Action::Notify(0),
                ],
            );
        });
}

#[test]
fn the_backoff_doubles_and_caps_at_the_cadence() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(Duration::from_secs(4))
                .opens_with(OpenScript::Failed(PassOutcome::NetworkFailure))
                .opens_with(OpenScript::Failed(PassOutcome::NetworkFailure))
                .opens_with(OpenScript::Failed(PassOutcome::NetworkFailure))
                .opens_with(OpenScript::Failed(PassOutcome::NetworkFailure))
                .passing(landed(0))
                .passing(landed(0))
                .passing(landed(0))
                .passing(landed(0))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            for _ in 0..4 {
                driver.step().await;
            }
            let waits: Vec<Duration> = host
                .actions()
                .iter()
                .filter_map(|action| match action {
                    Action::Wait(delay) => Some(*delay),
                    _ => None,
                })
                .collect();
            assert_eq!(
                waits,
                vec![
                    Duration::from_secs(1),
                    Duration::from_secs(2),
                    Duration::from_secs(4),
                    Duration::from_secs(4),
                ],
                "the backoff doubles from the base and caps at the cadence"
            );
        });
}

#[test]
fn a_standing_watch_resets_the_backoff() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(Duration::from_secs(8))
                .opens_with(OpenScript::Failed(PassOutcome::NetworkFailure))
                .opens_with(OpenScript::Watch)
                .opens_with(OpenScript::Watch)
                .watch_yields(Err(ProviderError::retryable("drop")))
                .passing(landed(0))
                .passing(landed(0))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await; // the failed open: 1s backoff
            driver.step().await; // the watch stands, the backoff resets
            driver.step().await; // the watch drops: the backoff starts from the base
            let waits: Vec<Duration> = host
                .actions()
                .iter()
                .filter_map(|action| match action {
                    Action::Wait(delay) => Some(*delay),
                    _ => None,
                })
                .collect();
            assert_eq!(
                waits,
                vec![Duration::from_secs(1), Duration::from_secs(1)],
                "a watch that stood is progress and resets the backoff"
            );
        });
}

#[test]
fn an_account_that_left_the_dial_table_stops_the_loop() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Gone)
                .clone();
            let mut driver = Driver::new(Arc::clone(&host));
            assert!(!driver.step().await, "the loop stops on a Gone answer");
            assert_eq!(
                host.actions(),
                vec![Action::Open],
                "a gone account runs no pass and waits no delay"
            );
        });
}

#[test]
fn a_standing_watch_stops_when_its_account_leaves_the_table() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            // The watch's own events are the only wake a parked watch gets,
            // so the liveness check rides them: an account removed
            // mid-watch drops its connection within one event, not never.
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Watch)
                .watch_yields(Ok(WatchEvent::KeepAlive))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await; // stand the watch
            host.presence(false);
            assert!(
                !driver.step().await,
                "the loop stops when the account left the dial table"
            );
            let actions = host.actions();
            assert!(
                !actions.contains(&Action::Pass(0)),
                "no pass runs for an account that left: {actions:?}"
            );
        });
}

#[test]
fn a_rate_limited_watch_drops_its_session_without_counting_an_outage() {
    crate::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            // A rate-limited server is reachable: the session drops and the
            // backoff runs, but the outage streak hears nothing.
            let host = FakeHost::polling(CADENCE)
                .opens_with(OpenScript::Watch)
                .watch_yields(Err(ProviderError::rate_limited("too many watches", None)))
                .passing(landed(0))
                .clone();

            let mut driver = Driver::new(Arc::clone(&host));
            driver.step().await; // stand the watch
            driver.step().await; // the rate limit: drop, back off, sync
            let actions = host.actions();
            assert!(
                !actions.contains(&Action::Record(PassOutcome::NetworkFailure)),
                "a rate limit is not an outage: {actions:?}"
            );
            assert_eq!(
                actions[1],
                Action::Wait(Duration::from_secs(1)),
                "it still backs off and syncs"
            );
        });
}
