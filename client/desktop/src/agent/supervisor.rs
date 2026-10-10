// SPDX-License-Identifier: GPL-3.0-only
//! Restart supervision for the sidecar relay. The relay's read pump
//! reports EOF and read errors here; this module owns the policy: stale
//! generation rejection, pending-prompt draining, the linear backoff
//! capped at [`BACKOFF_CAP_MS`], budget exhaustion into the Lost state
//! with the sidecar-lost event, and the delayed re-spawn.

use std::{sync::Arc, time::Duration};

use super::relay::RelayState;

/// The ceiling on one backoff wait: the schedule is linear
/// (`base * attempt`), so a base of one second waits one, two, three
/// seconds and then four, four, four.
pub(crate) const BACKOFF_CAP_MS: u64 = 4000;

/// The wait before the `attempt`th restart: linear in the attempt, capped.
pub(crate) fn backoff_for(base_ms: u64, attempt: u32) -> u64 {
    base_ms
        .saturating_mul(u64::from(attempt))
        .min(BACKOFF_CAP_MS)
}

/// Called by the relay when its connection dies: rejects stale reports,
/// drains everything in flight, fail-closes the asks, and either schedules
/// the next restart after the backoff or lands in the Lost state and says
/// so.
pub(crate) async fn handle_connection_dead(
    relay: &Arc<super::relay::SidecarRelay>,
    generation: u64,
) {
    let attempt = {
        let mut inner = relay.inner().lock().await;
        if inner.generation != generation {
            // A stale pump: this connection was already declared dead (or
            // replaced by a manual restart). Counting it would double-book
            // the budget.
            return;
        }
        inner.writer = None;
        inner.killer = None;
        let attempt = inner.attempts + 1;
        inner.attempts = attempt;
        // Invalidate this generation: a second report for the same
        // connection (a pump EOF racing a prompt-write failure) is then
        // stale too, never a double-counted death.
        inner.generation += 1;
        attempt
    };
    // The UI must never stick on thinking: one interrupted state and a
    // done per prompt.
    relay.drain_pending(true).await;
    // In-flight commands must not hang until their caller's timeout.
    relay.drain_commands().await;
    // Fail closed: unanswered approvals belong to the dead sidecar, and an
    // answer against the next one must never be writable.
    relay.fail_close_permissions("sidecar died");

    if attempt > relay.config().restart_max {
        relay.state_tx().send_replace(RelayState::Lost);
        relay
            .sink()
            .emit_sidecar_lost(attempt, "the sidecar connection was lost");
        return;
    }
    relay
        .state_tx()
        .send_replace(RelayState::Restarting { attempt });
    let backoff = backoff_for(relay.config().backoff_base_ms, attempt);
    let relay = Arc::clone(relay);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(backoff)).await;
        restart_after_backoff(relay, attempt).await;
    });
}

/// The wake-up after the backoff delay: only re-spawn while the state is
/// still the Restarting this schedule armed, so a manual restart or a
/// budget reset that won the race is respected.
async fn restart_after_backoff(relay: Arc<super::relay::SidecarRelay>, attempt: u32) {
    // Bound as a value first: a struct variant in an `if` condition would
    // parse as the condition's block.
    let expected = RelayState::Restarting { attempt };
    if relay.status() != expected {
        return;
    }
    // A start failure routes back through the death path, so the result
    // needs no handling here beyond the log.
    if let Err(e) = relay.start_once().await {
        log::warn!("agent: the sidecar restart {attempt} failed: {e}");
    }
}
