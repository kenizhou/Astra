// SPDX-License-Identifier: GPL-3.0-only
//! Restart supervision for one worker plugin. The pump reports EOF and read
//! errors here; this module owns the policy: stale generation rejection,
//! fail-closed draining of pending calls and subscriptions, the linear
//! backoff capped at four seconds, and budget exhaustion into
//! [`WorkerState::Lost`]. The shape mirrors the agent sidecar's supervisor.

use std::{sync::Arc, time::Duration};

use super::{spawn_and_init_once, Shared, WorkerError, WorkerState};

/// The ceiling on one backoff wait: the schedule is linear
/// (`base * attempt`), so a base of one second waits one, two, three seconds
/// and then four, four, four.
pub(crate) const BACKOFF_CAP: Duration = Duration::from_secs(4);

/// The wait before the `attempt`th restart: linear in the attempt, capped.
pub(crate) fn backoff_for(base: Duration, attempt: u32) -> Duration {
    base.saturating_mul(attempt).min(BACKOFF_CAP)
}

/// Called by the pump when its connection dies: rejects stale reports,
/// drains everything in flight fail-closed, and runs the retry loop until
/// the worker is Ready again or the budget is exhausted into Lost. The budget
/// check fires here, on the death itself: a restart that keeps succeeding and
/// re-dying must exhaust it, which a check inside the retry loop (on spawn
/// failure only) would never see.
pub(crate) async fn handle_dead(shared: &Arc<Shared>, generation: u64) {
    let attempt = {
        let mut inner = shared.inner.lock().await;
        if inner.generation != generation {
            // A stale pump: this connection was already declared dead (or
            // replaced by a manual restart). Counting it would double-book
            // the budget.
            return;
        }
        if matches!(shared.state(), WorkerState::Down | WorkerState::Lost) {
            // An orderly shutdown or an exhausted budget: no restart.
            return;
        }
        inner.writer = None;
        inner.killer = None;
        inner.generation += 1;
        inner.attempts += 1;
        // Fail closed: unanswered calls belong to the dead worker, and its
        // subscriptions drain on the same path its approvals would.
        for (_, pending) in inner.pending.drain() {
            let _ = pending.send(Err(WorkerError::Died));
        }
        inner.subscriptions.clear();
        inner.attempts
    };
    if attempt > shared.config.restart_max {
        shared.set_state(WorkerState::Lost);
        return;
    }
    shared.set_state(WorkerState::Restarting { attempt });
    let _ = retry_loop(shared).await;
}

/// The retry loop shared by the first start and every death: spawn and
/// handshake, and on failure count the attempt, sleep the backoff, and try
/// again until the budget exhausts into Lost. Fatal handshake errors (a
/// version mismatch, a worker serving undeclared methods) are never retried:
/// retrying them would spin the budget for nothing.
pub(crate) async fn retry_loop(shared: &Arc<Shared>) -> Result<(), WorkerError> {
    loop {
        if matches!(shared.state(), WorkerState::Down | WorkerState::Lost) {
            return Err(WorkerError::NotReady);
        }
        match spawn_and_init_once(shared).await {
            Ok(()) => {
                shared.set_state(WorkerState::Ready);
                return Ok(());
            }
            Err(error) => {
                let attempts = {
                    let mut inner = shared.inner.lock().await;
                    inner.attempts += 1;
                    inner.attempts
                };
                let fatal = matches!(
                    error,
                    WorkerError::VersionMismatch { .. } | WorkerError::UndeclaredMethods(_)
                );
                if fatal || attempts > shared.config.restart_max {
                    shared.set_state(WorkerState::Lost);
                    return Err(error);
                }
                shared.set_state(WorkerState::Restarting { attempt: attempts });
                tokio::time::sleep(backoff_for(shared.config.backoff_base, attempts)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_is_linear_and_capped() {
        let base = Duration::from_secs(1);
        assert_eq!(backoff_for(base, 1), Duration::from_secs(1));
        assert_eq!(backoff_for(base, 2), Duration::from_secs(2));
        assert_eq!(backoff_for(base, 3), Duration::from_secs(3));
        assert_eq!(backoff_for(base, 4), Duration::from_secs(4));
        assert_eq!(backoff_for(base, 9), Duration::from_secs(4));
    }
}
