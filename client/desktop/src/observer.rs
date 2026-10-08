// SPDX-License-Identifier: GPL-3.0-only
//! The observer port: [`ObserverBridge`] adapts the host's event sink to the app's
//! [`AppObserver`], and [`DebouncedObserver`] rate-limits the two surfaces a sync
//! burst drives to one notification each per window, ported from mail-calendar's
//! bindings.
//!
//! A streamed pass commits one chunk at a time, and each commit signals both
//! surfaces; without the rate limiter that is hundreds of notifications a second,
//! each one a full-snapshot pull and a repaint, on the thread the webview renders on.
//! The list [leads](takes_leading_edge): out of a quiet period it is forwarded at
//! once rather than at the window's end, because a user action produces exactly one
//! of those and has nothing to coalesce with. The progress bar deliberately does not:
//! a bar is meant to stay down for a download that finishes inside one window, and a
//! leading edge would raise it and take it away again on every short pass. Late is
//! right for a bar; late is wrong for the list.

use std::{
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use astra_app::{AppObserver, Surface};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// The event the host emits on every forwarded surface change, and the payload the
/// webview receives: which surface's snapshot to pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceChanged {
    /// The surface whose snapshot changed.
    pub surface: Surface,
}

/// Where a forwarded surface change lands: the Tauri event in the running app, a
/// channel in a test.
pub trait SurfaceEventSink: Send + Sync {
    /// One surface change, ready to cross to the webview.
    fn surface_changed(&self, change: SurfaceChanged);
}

/// The sink of the running app: emits the `surface_changed` event to every webview
/// the shell holds.
#[derive(Debug)]
pub struct TauriEventSink {
    /// The handle the event is emitted on.
    handle: AppHandle,
}

impl TauriEventSink {
    /// A sink emitting on `handle`.
    pub fn new(handle: AppHandle) -> Self {
        Self { handle }
    }
}

impl SurfaceEventSink for TauriEventSink {
    fn surface_changed(&self, change: SurfaceChanged) {
        if let Err(err) = self.handle.emit("surface_changed", change) {
            log::warn!("observer: a surface change could not be emitted: {err}");
        }
    }
}

/// Adapts a [`SurfaceEventSink`] to the app's [`AppObserver`].
pub(crate) struct ObserverBridge {
    /// The host's own notification path.
    sink: Box<dyn SurfaceEventSink>,
}

impl ObserverBridge {
    /// A bridge over `sink`.
    pub(crate) fn new(sink: Box<dyn SurfaceEventSink>) -> Self {
        Self { sink }
    }
}

impl AppObserver for ObserverBridge {
    fn surface_changed(&self, surface: Surface) {
        self.sink.surface_changed(SurfaceChanged { surface });
    }
}

/// How long a [`DebouncedObserver`] window lasts: one notification per surface per
/// window, plus the leading one for the surfaces that [take it](takes_leading_edge).
///
/// 250 ms keeps the UI feeling live (~4 renders/s while a sync streams) while
/// eliminating the hundreds-per-second churn that made the list flicker. It bounds a
/// burst, and deliberately does not delay the signal that starts one.
const DEBOUNCE_MS: u64 = 250;

/// Returns `true` for the surfaces a sync burst drives, which are coalesced rather
/// than forwarded on every commit.
///
/// Both of them, and the pairing is the point: a commit signals the list (the
/// spliced pane) and then the progress bar; debouncing only the first achieved
/// nothing while the bar ran undebounced beside it.
fn is_debounced(surface: Surface) -> bool {
    matches!(surface, Surface::MailboxList | Surface::SyncProgress)
}

/// Returns `true` for a debounced surface whose first signal out of a quiet period
/// is worth forwarding straight away rather than at the window's end.
///
/// Only the list: it is what a user action changes, and holding that signal would put
/// the window in front of every navigation.
fn takes_leading_edge(surface: Surface) -> bool {
    matches!(surface, Surface::MailboxList)
}

#[derive(Debug, Default)]
struct DebouncedState {
    /// The debounced surfaces awaiting delivery; at most one entry each, which is
    /// what makes a burst one notification.
    pending: Vec<Surface>,
    /// When the earliest pending surface's window closes, and so when the drain
    /// thread should next look.
    deadline: Option<Instant>,
    /// The open rate-limit window per surface. Per surface rather than shared, so a
    /// sync ticking the bar cannot hold the window the list's leading edge tests.
    windows: Vec<(Surface, Instant)>,
}

impl DebouncedState {
    /// Records `surface` and answers whether the caller should forward it now.
    ///
    /// A surface that [takes a leading edge](takes_leading_edge) and arrives out of
    /// a quiet period is forwarded immediately; everything else is queued for its
    /// window's end, which is what collapses a streamed sync to one notification per
    /// surface.
    fn mark(&mut self, surface: Surface, now: Instant) -> bool {
        if self.window_end(surface).is_none_or(|ends| ends <= now) {
            self.open_window(surface, now);
            if takes_leading_edge(surface) {
                return true;
            }
        }
        if !self.pending.contains(&surface) {
            self.pending.push(surface);
        }
        // Always the window's end, never `now + DEBOUNCE_MS`: pushing the deadline
        // forward on every signal meant a pass that commits continuously never
        // reached it at all.
        self.refresh_deadline();
        false
    }

    /// Puts the deadline on the earliest window a queued surface is waiting for.
    fn refresh_deadline(&mut self) {
        self.deadline = self
            .pending
            .iter()
            .filter_map(|surface| self.window_end(*surface))
            .min();
    }

    /// The end of `surface`'s open window, if it has one.
    fn window_end(&self, surface: Surface) -> Option<Instant> {
        self.windows
            .iter()
            .find(|(open, _)| *open == surface)
            .map(|(_, ends)| *ends)
    }

    /// Starts `surface`'s next window, replacing any window it already had.
    fn open_window(&mut self, surface: Surface, now: Instant) {
        let ends = now + Duration::from_millis(DEBOUNCE_MS);
        if let Some((_, open)) = self.windows.iter_mut().find(|(open, _)| *open == surface) {
            *open = ends;
        } else {
            self.windows.push((surface, ends));
        }
    }

    /// Takes the queued surfaces whose window has closed, opening each one's next
    /// window, and leaves the deadline on the earliest still waiting.
    fn take_due(&mut self, now: Instant) -> Vec<Surface> {
        let due: Vec<Surface> = self
            .pending
            .iter()
            .copied()
            .filter(|surface| self.window_end(*surface).is_none_or(|ends| ends <= now))
            .collect();
        self.pending.retain(|surface| !due.contains(surface));
        for surface in &due {
            self.open_window(*surface, now);
        }
        self.refresh_deadline();
        due
    }

    /// Takes everything queued whatever its window, clearing the deadline.
    fn take(&mut self) -> Vec<Surface> {
        self.deadline = None;
        std::mem::take(&mut self.pending)
    }
}

/// Wraps an [`ObserverBridge`] and rate-limits the debounced surfaces: everything
/// arriving while a surface's window is open collapses into one notification at its
/// end, so a streamed sync becomes a few re-renders instead of hundreds. A surface
/// that [leads](takes_leading_edge) is forwarded at once when it arrives out of a
/// quiet period, so the window never lands in front of a user action. Any other
/// surface is forwarded immediately and flushes what is pending first, so the host
/// always sees a consistent state.
pub(crate) struct DebouncedObserver {
    inner: Arc<ObserverBridge>,
    state: Arc<(Mutex<DebouncedState>, Condvar)>,
}

impl DebouncedObserver {
    /// Builds a [`DebouncedObserver`] and spawns the background drain thread.
    pub(crate) fn new(bridge: ObserverBridge) -> Self {
        let inner = Arc::new(bridge);
        let state: Arc<(Mutex<DebouncedState>, Condvar)> =
            Arc::new((Mutex::new(DebouncedState::default()), Condvar::new()));
        let drain_inner = Arc::clone(&inner);
        let drain_state = Arc::clone(&state);
        thread::Builder::new()
            .name("astra-observer-debounce".to_owned())
            .spawn(move || Self::drain_loop(&drain_inner, &drain_state))
            .expect("the debounce drain thread spawns");
        Self { inner, state }
    }

    /// The background thread: sleeps until the next deadline, then fires the pending
    /// signals.
    fn drain_loop(inner: &ObserverBridge, state: &(Mutex<DebouncedState>, Condvar)) {
        let (lock, cvar) = state;
        loop {
            // Wait until there is a deadline to fire.
            let deadline = {
                let guard = lock.lock().expect("debounce mutex not poisoned");
                let guard = cvar
                    .wait_while(guard, |s| s.deadline.is_none())
                    .expect("debounce condvar not poisoned");
                guard.deadline.expect("deadline is set when we wake")
            };

            // Sleep until the deadline (or immediately if it already passed).
            if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                thread::sleep(remaining);
            }

            // Fire only if the deadline has not been pushed forward or drained while
            // we slept.
            let should_fire = {
                let mut guard = lock.lock().expect("debounce mutex not poisoned");
                if guard
                    .deadline
                    .is_some_and(|d| d.checked_duration_since(Instant::now()).is_some())
                {
                    Vec::new()
                } else {
                    guard.take_due(Instant::now())
                }
            };

            for surface in should_fire {
                inner.surface_changed(surface);
            }
        }
    }

    /// Immediately fires every queued debounced signal. Called before forwarding any
    /// non-debounced signal so the host always sees a consistent state; both surfaces
    /// this milestone ships are debounced, so it stands ready for the first one that
    /// is not.
    fn flush_pending(&self) {
        let (lock, _) = &*self.state;
        let pending = {
            let mut guard = lock.lock().expect("debounce mutex not poisoned");
            guard.take()
        };
        for surface in pending {
            self.inner.surface_changed(surface);
        }
    }
}

impl AppObserver for DebouncedObserver {
    fn surface_changed(&self, surface: Surface) {
        if is_debounced(surface) {
            let (lock, cvar) = &*self.state;
            let forward_now = {
                let mut guard = lock.lock().expect("debounce mutex not poisoned");
                let forward_now = guard.mark(surface, Instant::now());
                if !forward_now {
                    cvar.notify_one();
                }
                forward_now
            };
            // Outside the lock: the sink emits on this call, and holding the mutex
            // across it would put every other signalling task behind the emit.
            if forward_now {
                self.inner.surface_changed(surface);
            }
        } else {
            self.flush_pending();
            self.inner.surface_changed(surface);
        }
    }
}

#[cfg(test)]
#[path = "observer_tests.rs"]
mod tests;
