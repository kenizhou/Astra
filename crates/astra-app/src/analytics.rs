// SPDX-License-Identifier: GPL-3.0-only
//! The analytics recorder: the typed event the host reports and the
//! consent gate it passes through, landing as counts-and-shapes rows in
//! the AI ledger beside the tool-call and LLM-call ledgers. Nothing leaves
//! the device: the sink is the local store, and no endpoint exists.
//!
//! The consent gate reads the persisted preference live on every call, so
//! the one-click withdrawal (the preference write itself) stops the flow
//! at the next call, and a consent that never stood records nothing at
//! all: no row, no error, the recorder simply answers nothing.

use std::{collections::BTreeMap, sync::Arc};

use astra_ai::{AiStore, NewAnalyticsEvent};
use engine_api::Provider;

use crate::App;

/// One analytics event the host reports: a shape name and named counts,
/// never content. The shape names come from the host's own closed
/// vocabulary, and the counts are integers by type, so no subject,
/// address, or body can ride a value; the row shape itself is what keeps
/// it so, the same rule the tool-call ledger's column set follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsEvent {
    /// The event's shape name, for example `sync_pass`.
    pub shape: String,
    /// The event's named counts, for example `{"messages": 12}`.
    pub counts: BTreeMap<String, u64>,
}

impl AnalyticsEvent {
    /// An event of `shape` with no counts, the shape a bare occurrence
    /// reports.
    #[must_use]
    pub fn of(shape: impl Into<String>) -> Self {
        Self {
            shape: shape.into(),
            counts: BTreeMap::new(),
        }
    }

    /// Adds one named count, the builder the host's one-line reports read.
    #[must_use]
    pub fn with_count(mut self, name: impl Into<String>, value: u64) -> Self {
        self.counts.insert(name.into(), value);
        self
    }
}

impl<P: Provider> App<P> {
    /// Installs the ledger the consented events append to. Call it before
    /// the first record: an app with no ledger installed records nothing,
    /// the same shape an app with no account store renders no rows with.
    pub fn install_analytics_store(&self, store: Arc<AiStore>) {
        *self.analytics.lock().expect("analytics mutex poisoned") = Some(store);
    }

    /// Records one analytics event. The row lands only while consent
    /// stands: the gate reads the persisted preference on every call, so
    /// the withdrawal stops the flow at the next call and a consent that
    /// never stood records nothing, no row and no error. A ledger write
    /// that fails logs and the host's flow moves on: the recorder never
    /// fails a path the person is in the middle of.
    pub fn record_analytics(&self, event: &AnalyticsEvent) {
        let consented = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .prefs
            .analytics_consented;
        if !consented {
            return;
        }
        let Some(store) = self
            .analytics
            .lock()
            .expect("analytics mutex poisoned")
            .clone()
        else {
            return;
        };
        let row = NewAnalyticsEvent {
            shape: event.shape.clone(),
            counts: event.counts.clone(),
        };
        if let Err(err) = store.record_analytics_event(&row) {
            log::warn!("analytics: the event row could not land: {err}");
        }
    }
}

#[cfg(test)]
#[path = "analytics_tests.rs"]
mod tests;
