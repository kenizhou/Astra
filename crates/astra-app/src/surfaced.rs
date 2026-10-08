// SPDX-License-Identifier: GPL-3.0-only
//! A snapshot a host pulls, and the signal that tells it to: as one thing.
//!
//! The hazard this closes is an ordering one, and it is invisible in review. A host learns
//! a surface changed from [`AppObserver::surface_changed`] and then *pulls* the snapshot;
//! if the signal goes out before the snapshot is written, the host reads the previous one
//! and paints stale. Nothing then corrects it; there is no second signal coming, because
//! as far as the core is concerned it already told the host.
//!
//! The observer is *inside* the cell, so [`publish`](Surfaced::publish) is the only way to
//! store a value and it always signals afterwards. There is no method that writes without
//! signalling and none that signals a value it has not stored: the pull,
//! [`get`](Surfaced::get), signals nothing at all.
//!
//! [`resignal`](Surfaced::resignal) is the deliberate exception, and it is safe by
//! construction: it sends the signal for a value that is *already* published, so there is
//! nothing for it to be ahead of. A cold launch re-announcing an already-primed list is
//! the case that needs it: the list was primed before the host had wired its observer.

use std::sync::{Arc, Mutex};

use crate::{AppObserver, Surface};

/// A snapshot the host pulls, its surface, and the observer to announce it on.
///
/// `T` is cloned out on every pull, so it is one of the view-model snapshot types: small,
/// owned, and cheap to hand across the IPC.
pub struct Surfaced<T> {
    value: Mutex<T>,
    surface: Surface,
    observer: Arc<dyn AppObserver>,
}

impl<T> core::fmt::Debug for Surfaced<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The value may hold the user's mail; the surface alone is enough to identify the cell.
        f.debug_struct("Surfaced")
            .field("surface", &self.surface)
            .finish_non_exhaustive()
    }
}

impl<T: Clone + Default> Surfaced<T> {
    /// An unpublished cell holding `T::default()`, bound to `surface` and `observer`.
    pub fn new(surface: Surface, observer: Arc<dyn AppObserver>) -> Self {
        Self {
            value: Mutex::new(T::default()),
            surface,
            observer,
        }
    }
}

impl<T: Clone> Surfaced<T> {
    /// Stores `value` and then announces it. The store happens first, always: a host that
    /// pulls the instant it is signalled gets this value and not the one before it.
    pub fn publish(&self, value: T) {
        *self.value.lock().expect("surfaced mutex poisoned") = value;
        self.observer.surface_changed(self.surface);
    }

    /// The current value, for the host's pull.
    pub fn get(&self) -> T {
        self.value.lock().expect("surfaced mutex poisoned").clone()
    }

    /// Announces what is **already** published, without writing.
    ///
    /// For the case where the thing that went stale is not this snapshot: a cold launch
    /// whose primed list was signalled before the host had wired its observer. It cannot
    /// reintroduce the ordering bug; there is no new value for the signal to run ahead of.
    pub fn resignal(&self) {
        self.observer.surface_changed(self.surface);
    }
}

#[cfg(test)]
#[path = "surfaced_tests.rs"]
mod tests;
