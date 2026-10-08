// SPDX-License-Identifier: GPL-3.0-only
//! A plugin's lifecycle state machine.
//!
//! The legal graph, expressed once here and enforced by [`Lifecycle::transition`]:
//!
//! ```text
//! Discovered --Load--> Loaded --Activate--> Activated
//!      |                   |                    |
//!      +-------------------+--------------------+--Fail(reason)--> Failed
//! ```
//!
//! In words: `Discovered` moves to `Loaded` on [`LifecycleEvent::Load`], `Loaded`
//! moves to `Activated` on [`LifecycleEvent::Activate`], and every live state
//! moves to [`Lifecycle::Failed`] on [`LifecycleEvent::Fail`]. `Failed` is
//! terminal: no event leaves it, so a failed plugin is never resurrected through
//! this machine (the host discovers a fresh instance instead). Every other
//! pairing of state and event, including a repeated `Load` or `Activate`, is
//! refused with [`IllegalTransition`].

use std::{error::Error, fmt};

/// The states a plugin passes through between the host finding it and it serving
/// hooks. The states carry no process handles; supervision arrives with the
/// out-of-process host and builds on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifecycle {
    /// The manifest is found and parsed; the plugin's code is not yet loaded.
    Discovered,
    /// The plugin's code is loaded and its hooks registered; it serves nothing yet.
    Loaded,
    /// The plugin is serving its declared hooks.
    Activated,
    /// The plugin failed for the carried reason and will not run again.
    Failed(String),
}

/// The events that drive a [`Lifecycle`] forward or into failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// Load a discovered plugin's code.
    Load,
    /// Activate a loaded plugin so it starts serving hooks.
    Activate,
    /// Fail the plugin for the carried reason; legal from any state except
    /// [`Lifecycle::Failed`], where it is a no-op the machine refuses.
    Fail(String),
}

/// A state-and-event pairing the lifecycle graph does not allow. Carries both
/// sides so a supervisor logging it says what was attempted from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalTransition {
    /// The state the transition was attempted from.
    pub from: Lifecycle,
    /// The event that was refused.
    pub event: LifecycleEvent,
}

impl fmt::Display for IllegalTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "illegal lifecycle transition from {:?} on {:?}",
            self.from, self.event
        )
    }
}

impl Error for IllegalTransition {}

impl Lifecycle {
    /// Applies `event` to this state, returning the state it produces.
    ///
    /// # Errors
    ///
    /// Returns [`IllegalTransition`] for every pairing outside the graph in this
    /// module's docs: activating before loading, repeating `Load` or `Activate`,
    /// and any event out of [`Lifecycle::Failed`].
    pub fn transition(self, event: LifecycleEvent) -> Result<Self, IllegalTransition> {
        match (self, event) {
            (Lifecycle::Discovered, LifecycleEvent::Load) => Ok(Lifecycle::Loaded),
            (Lifecycle::Loaded, LifecycleEvent::Activate) => Ok(Lifecycle::Activated),
            (
                Lifecycle::Discovered | Lifecycle::Loaded | Lifecycle::Activated,
                LifecycleEvent::Fail(reason),
            ) => Ok(Lifecycle::Failed(reason)),
            (from, event) => Err(IllegalTransition { from, event }),
        }
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
