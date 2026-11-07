// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the calendar refresh timer over a fake host: a tick waits the
//! cadence then refreshes while an account stands connected, a tick with
//! nothing connected reaches no provider, and the limit stops the timer
//! after its ticks.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;

use super::{calendar_refresh_interval, parse_calendar_refresh_limit, CalendarTimer, TimerHost};

/// One recorded action of the fake host, the log the assertions read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// The timer waited out one tick.
    Wait(Duration),
    /// The timer ran one refresh.
    Refresh,
}

/// The fake host: connected scripted, actions recorded.
struct FakeHost {
    /// Whether the connected gate stands open.
    connected: bool,
    /// Every action, in order.
    actions: Arc<Mutex<Vec<Action>>>,
}

#[async_trait]
impl TimerHost for Arc<FakeHost> {
    async fn wait(&self, delay: Duration) {
        self.actions.lock().unwrap().push(Action::Wait(delay));
    }

    fn connected(&self) -> bool {
        self.connected
    }

    async fn refresh(&self) {
        self.actions.lock().unwrap().push(Action::Refresh);
    }
}

/// A timer over a fake host with `connected` scripted, recording actions.
fn timer(connected: bool) -> (CalendarTimer<Arc<FakeHost>>, Arc<Mutex<Vec<Action>>>) {
    let actions = Arc::new(Mutex::new(Vec::new()));
    let host = Arc::new(FakeHost {
        connected,
        actions: Arc::clone(&actions),
    });
    (CalendarTimer::with_limit(host, None), actions)
}

/// Drives one async test body on the host's runtime: this crate forbids
/// `#[tokio::test]`'s `block_on` by clippy config, so the tests block on
/// their own runtime the way the boot tests do.
fn drive(
    make: impl FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send,
) {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(make);
}

#[test]
fn a_tick_waits_the_cadence_then_refreshes_while_an_account_is_connected() {
    drive(|| {
        Box::pin(async {
            let (mut timer, actions) = timer(true);

            assert!(timer.step().await, "an unbounded timer continues");

            assert_eq!(
                actions.lock().unwrap().as_slice(),
                [Action::Wait(calendar_refresh_interval()), Action::Refresh],
                "one tick waits the cadence out, then reaches the refresh body"
            );
        })
    });
}

#[test]
fn a_tick_with_nothing_connected_refreshes_nothing() {
    drive(|| {
        Box::pin(async {
            let (mut timer, actions) = timer(false);

            assert!(timer.step().await);

            assert_eq!(
                actions.lock().unwrap().as_slice(),
                [Action::Wait(calendar_refresh_interval())],
                "the gate the delivery supervisor's adoption reads holds the tick's \
             refresh back"
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
                connected: true,
                actions: Arc::clone(&actions),
            });
            let mut timer = CalendarTimer::with_limit(host, Some(2));

            assert!(timer.step().await, "the first of two ticks continues");
            assert!(!timer.step().await, "the second of two ticks is the last");
            assert_eq!(
                actions.lock().unwrap().as_slice(),
                [
                    Action::Wait(calendar_refresh_interval()),
                    Action::Refresh,
                    Action::Wait(calendar_refresh_interval()),
                    Action::Refresh,
                ],
                "exactly the limited number of refreshes ran"
            );
        })
    });
}

#[test]
fn the_refresh_limit_accepts_only_a_positive_integer() {
    assert_eq!(parse_calendar_refresh_limit(Some("2")), Some(2));
    assert_eq!(parse_calendar_refresh_limit(Some("0")), None);
    assert_eq!(parse_calendar_refresh_limit(Some("many")), None);
    assert_eq!(parse_calendar_refresh_limit(None), None);
}

#[test]
fn the_interval_defaults_to_five_minutes() {
    assert_eq!(calendar_refresh_interval(), Duration::from_secs(300));
}
