// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the snooze wake timer over a fake host: a tick waits the cadence
//! then wakes while an entry is due, a tick with nothing due wakes nothing,
//! and the limit stops the timer after its ticks. The fake clock scripts the
//! instant and the due gate, so no real time passes.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;

use super::{snooze_interval, SnoozeHost, SnoozeTimer};

// Each override helper joins its source's gate: the parser is debug-only,
// and the release-only assertion is the limit read's only user.
#[cfg(debug_assertions)]
use super::parse_snooze_limit;
#[cfg(not(debug_assertions))]
use super::snooze_limit;

/// One recorded action of the fake host, the log the assertions read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// The timer waited out one tick.
    Wait(Duration),
    /// The timer ran one wake at the instant.
    Wake(i64),
}

/// The fake host: the instant and the due gate scripted, actions recorded.
struct FakeHost {
    /// The instant the tick wakes at.
    now: i64,
    /// Whether the due gate stands open.
    due: bool,
    /// Every action, in order.
    actions: Arc<Mutex<Vec<Action>>>,
}

#[async_trait]
impl SnoozeHost for Arc<FakeHost> {
    async fn wait(&self, delay: Duration) {
        self.actions.lock().unwrap().push(Action::Wait(delay));
    }

    fn now(&self) -> i64 {
        self.now
    }

    fn due(&self, _now: i64) -> bool {
        self.due
    }

    async fn wake(&self, now: i64) {
        self.actions.lock().unwrap().push(Action::Wake(now));
    }
}

/// A timer over a fake host with the instant and due gate scripted.
fn timer(now: i64, due: bool) -> (SnoozeTimer<Arc<FakeHost>>, Arc<Mutex<Vec<Action>>>) {
    let actions = Arc::new(Mutex::new(Vec::new()));
    let host = Arc::new(FakeHost {
        now,
        due,
        actions: Arc::clone(&actions),
    });
    (SnoozeTimer::with_limit(host, None), actions)
}

/// Drives one async test body on the host's runtime: this crate forbids
/// `#[tokio::test]`'s `block_on` by clippy config, so the tests block on
/// their own runtime the way the calendar timer tests do.
fn drive(
    make: impl FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send,
) {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(make);
}

#[test]
fn a_tick_waits_the_cadence_then_wakes_when_an_entry_is_due() {
    drive(|| {
        Box::pin(async {
            let (mut timer, actions) = timer(500, true);

            assert!(timer.step().await, "an unbounded timer continues");

            assert_eq!(
                actions.lock().unwrap().as_slice(),
                [Action::Wait(snooze_interval()), Action::Wake(500)],
                "one tick waits the cadence out, then wakes at the scripted instant"
            );
        })
    });
}

#[test]
fn a_tick_with_nothing_due_wakes_nothing() {
    drive(|| {
        Box::pin(async {
            let (mut timer, actions) = timer(500, false);

            assert!(timer.step().await);

            assert_eq!(
                actions.lock().unwrap().as_slice(),
                [Action::Wait(snooze_interval())],
                "the due gate holds the tick's wake back"
            );
        })
    });
}

#[test]
fn the_limit_stops_the_timer_after_its_ticks() {
    drive(|| {
        Box::pin(async {
            let actions = Arc::new(Mutex::new(Vec::new()));
            let host = Arc::new(FakeHost {
                now: 500,
                due: true,
                actions: Arc::clone(&actions),
            });
            let mut timer = SnoozeTimer::with_limit(host, Some(2));

            assert!(timer.step().await, "the first of two ticks continues");
            assert!(!timer.step().await, "the second of two ticks is the last");
            assert_eq!(
                actions.lock().unwrap().as_slice(),
                [
                    Action::Wait(snooze_interval()),
                    Action::Wake(500),
                    Action::Wait(snooze_interval()),
                    Action::Wake(500),
                ],
                "exactly the limited number of wakes ran"
            );
        })
    });
}

#[test]
fn the_standing_limit_is_none_and_the_cadence_constant_is_one_minute() {
    // The constant, not the interval fn: a test process that exported the
    // debug override would otherwise fail the one-minute pin.
    assert_eq!(super::SNOOZE_TICK, Duration::from_secs(60));
    #[cfg(not(debug_assertions))]
    {
        assert_eq!(snooze_limit(), None, "release holds no limit");
    }
}

#[cfg(debug_assertions)]
#[test]
fn the_snooze_limit_accepts_only_a_positive_integer() {
    assert_eq!(parse_snooze_limit(Some("2")), Some(2));
    assert_eq!(parse_snooze_limit(Some("0")), None);
    assert_eq!(parse_snooze_limit(Some("many")), None);
    assert_eq!(parse_snooze_limit(None), None);
}
