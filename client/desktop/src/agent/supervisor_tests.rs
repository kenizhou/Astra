// SPDX-License-Identifier: GPL-3.0-only
//! The supervisor policy: the linear capped backoff schedule, the stale
//! generation rejection, the budget exhausting into the Lost state with
//! its sidecar-lost event, and the recovery of a Lost relay through a
//! fresh start. The tests block on their own runtime (the host's drive
//! pattern): this crate forbids `#[tokio::test]`'s `block_on` by clippy
//! config.

use std::sync::Arc;
use std::time::Duration;

use super::{
    supervisor::{backoff_for, handle_connection_dead, BACKOFF_CAP_MS},
    testkit::{stub_relay, stub_relay_with, CollectingSink},
};
use astra_ai::CorrelationId;

/// One test's runtime, built the way the host builds its own.
fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
}

/// Waits for the predicate, up to five seconds.
async fn until<T>(mut read: impl FnMut() -> T, want: impl Fn(&T) -> bool) -> T {
    for _ in 0..1000 {
        let current = read();
        if want(&current) {
            return current;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    read()
}

#[test]
fn the_backoff_is_linear_then_capped() {
    assert_eq!(backoff_for(1000, 1), 1000, "attempt one waits the base");
    assert_eq!(
        backoff_for(1000, 2),
        2000,
        "attempt two waits twice the base"
    );
    assert_eq!(
        backoff_for(1000, 3),
        3000,
        "attempt three waits thrice the base"
    );
    assert_eq!(
        backoff_for(1000, 4),
        BACKOFF_CAP_MS,
        "the cap holds from four on"
    );
    assert_eq!(backoff_for(1000, 50), BACKOFF_CAP_MS);
    assert_eq!(
        backoff_for(50, 3),
        150,
        "a small base stays linear under the cap"
    );
}

#[test]
fn the_budget_exhausts_into_lost_with_the_event() {
    let runtime = runtime();
    // Two allowed restarts: deaths one and two schedule restarts, the
    // third death exceeds the budget and lands Lost.
    let (relay, sink, _data) = stub_relay("lost", "die", 2, 20);
    runtime
        .block_on(|| async { relay.start_once().await })
        .expect("the first spawn");

    let final_state = runtime.block_on(|| async {
        until(
            || relay.status(),
            |state| matches!(state, super::relay::RelayState::Lost),
        )
        .await
    });
    assert_eq!(
        final_state.as_str(),
        "lost",
        "three deaths exhaust a budget of two"
    );

    let lost = sink.lost.lock().unwrap();
    assert_eq!(lost.len(), 1, "exactly one lost emission");
    assert_eq!(lost[0].0, 3, "the event carries the attempt count");
    drop(lost);
    assert_eq!(runtime.block_on(|| async { relay.attempts().await }), 3);
}

#[test]
fn a_stale_generation_report_is_ignored() {
    let runtime = runtime();
    // A long backoff keeps the scheduled restart from racing the test: the
    // one death and its bookkeeping are all that move.
    let (relay, sink, _data) = stub_relay("stale", "die", 5, 10_000);
    runtime
        .block_on(|| async { relay.start_once().await })
        .expect("the first spawn");

    // The death lands the relay in Restarting and invalidates generation 1.
    let restarting = runtime.block_on(|| async {
        until(
            || relay.status(),
            |state| matches!(state, super::relay::RelayState::Restarting { .. }),
        )
        .await
    });
    let super::relay::RelayState::Restarting { attempt } = restarting else {
        unreachable!("the until above guarantees Restarting");
    };
    assert_eq!(attempt, 1);
    assert_eq!(runtime.block_on(|| async { relay.attempts().await }), 1);

    // The dead connection's generation reporting again (a pump EOF racing
    // a write failure) must not count a second death.
    let dead_generation = runtime.block_on(|| async { relay.generation().await }) - 1;
    runtime.block_on(|| async { handle_connection_dead(&relay, dead_generation).await });
    assert_eq!(
        runtime.block_on(|| async { relay.attempts().await }),
        1,
        "the stale report counts nothing"
    );
    assert!(matches!(
        relay.status(),
        super::relay::RelayState::Restarting { .. }
    ));
    assert!(
        sink.lost.lock().unwrap().is_empty(),
        "no lost emission for a stale report"
    );
}

#[test]
fn a_lost_relay_recovers_through_a_fresh_start() {
    let runtime = runtime();
    // The rotating spawner: the first three spawns die instantly (a
    // budget of two exhausts on the third death), the fourth runs the
    // well-behaved echo stub the fresh start recovers onto.
    let (relay, sink, _data) =
        stub_relay_with("recover", &["die", "die", "die", "echo"], &[], 2, 20);
    runtime
        .block_on(|| async { relay.start_once().await })
        .expect("the first spawn");
    let state = runtime.block_on(|| async {
        until(
            || relay.status(),
            |state| matches!(state, super::relay::RelayState::Lost),
        )
        .await
    });
    assert_eq!(state.as_str(), "lost");

    // A prompt asks for a ready sidecar: ensure_started resets the budget
    // and spawns the live stub, and the turn completes.
    let id = runtime
        .block_on(|| async {
            relay
                .prompt("back again", Some(CorrelationId::new("corr-11".to_owned())))
                .await
        })
        .expect("the fresh start recovers");
    assert_eq!(relay.status().as_str(), "ready");
    assert_eq!(
        runtime.block_on(|| async { relay.attempts().await }),
        0,
        "the fresh start reset the budget"
    );

    let events = runtime.block_on(|| async { sink.await_events(3).await });
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            super::StreamPayload::Done { prompt_id } if prompt_id == &id
        )),
        "the recovered relay completes the turn: {events:?}"
    );
}

#[test]
fn a_death_fail_closes_the_pending_asks() {
    let runtime = runtime();
    let (relay, sink, _data): (Arc<super::relay::SidecarRelay>, Arc<CollectingSink>, _) =
        stub_relay("failclose", "die", 5, 20);
    runtime
        .block_on(|| async { relay.start_once().await })
        .expect("the first spawn");

    // Simulate an ask the dying sidecar never got an answer for: the
    // supervisor's death path must audit it as closed and tell the UI.
    relay.register_test_approval(super::permissions::PendingApproval {
        id: "perm-orphan".to_owned(),
        tool_call_id: "call-x".to_owned(),
        tool: "save_draft".to_owned(),
        level: super::protocol::PermissionLevel::Confirm,
        args: serde_json::json!({ "account": "acct-1" }),
        preview: None,
        correlation: Some("corr-12".to_owned()),
    });

    let _ = runtime.block_on(|| async {
        until(
            || relay.status(),
            |state| matches!(state, super::relay::RelayState::Restarting { .. }),
        )
        .await
    });
    let resolved = sink.resolved.lock().unwrap();
    assert!(
        resolved
            .iter()
            .any(|settle| settle.id == "perm-orphan" && !settle.approved),
        "the orphaned ask is fail-closed to the UI: {resolved:?}"
    );
    assert!(
        relay.pending_approvals().is_empty(),
        "nothing is answerable against the next sidecar"
    );
}
