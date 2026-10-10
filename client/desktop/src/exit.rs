// SPDX-License-Identifier: GPL-3.0-only
//! The exit hook: reached exactly once on every exit path, after the windows are
//! gone but while the managed state is still alive, it shuts the agent sidecar
//! down fail-closed and stops background sync with a bound.

use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent};

use crate::AppState;

/// How long exiting waits for in-flight sync before it stops waiting: long enough
/// for a chunk commit to land (the unit a streamed pass costs), short enough that
/// closing the window never feels like it hung.
const EXIT_SYNC_BOUND: Duration = Duration::from_secs(5);

/// The run-event handler: on exit, stops the managed state's sync.
// `.run()` hands the event by value; the hook inspects what it owns.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn on_run_event(app: &AppHandle, event: RunEvent) {
    if let RunEvent::Exit = event {
        if let Some(state) = app.try_state::<AppState>() {
            state.stop_sync();
        }
    }
}

impl AppState {
    /// Stops the host's background work: the agent sidecar first (killed,
    /// its in-flight prompts settled, its unanswered approvals fail-closed
    /// rather than approved by disappearance), then background sync, with
    /// in-flight work bounded by [`EXIT_SYNC_BOUND`]. Everything the
    /// runtime still owes (a commit mid-write, a first sync) is either
    /// finished inside the bound or picked up by the next launch's lease
    /// recovery; the process does not outlive the window closing.
    pub(crate) fn stop_sync(&self) {
        self.runtime.block_on(|| self.agent.shutdown());
        self.runtime.shutdown_timeout(EXIT_SYNC_BOUND);
    }
}
