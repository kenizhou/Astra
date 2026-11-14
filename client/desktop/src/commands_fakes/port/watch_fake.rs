// SPDX-License-Identifier: GPL-3.0-only
//! The watch fake: the recorded asks and the preset answers, beside the
//! [`WatchPort`] impl that serves them. A distinct trait from `AppPort`, so
//! its impl lives in its own child of the port module (one trait's impl
//! cannot split; distinct traits impl freely), keeping the port file under
//! the line cap.

use astra_app::{NewMailNotice, RowRef};
use astra_watches::WatchEntry;
use async_trait::async_trait;

use crate::app_state::WatchPort;
use crate::commands_fakes::FakeApp;

/// The watch fake's state: the preset answers and the recorded asks.
#[derive(Default)]
pub(crate) struct WatchFake {
    /// The entries the listing answers with.
    pub(crate) entries: Vec<WatchEntry>,
    /// How many times the listing was pulled.
    pub(crate) listed: usize,
    /// The rows the watch was asked for.
    pub(crate) watched: Vec<RowRef>,
    /// The rows the unwatch was asked for.
    pub(crate) unwatched: Vec<RowRef>,
    /// The batch sizes the reply check was asked about.
    pub(crate) checked: Vec<usize>,
    /// The notices the reply check answers with.
    pub(crate) replies: Vec<NewMailNotice>,
}

#[async_trait]
impl WatchPort for FakeApp {
    async fn watch_thread(&self, row: RowRef) -> Result<bool, String> {
        self.watches.lock().unwrap().watched.push(row);
        Ok(true)
    }

    async fn unwatch_thread(&self, row: RowRef) -> Result<bool, String> {
        self.watches.lock().unwrap().unwatched.push(row);
        Ok(true)
    }

    async fn list_watches(&self) -> Result<Vec<WatchEntry>, String> {
        let mut fake = self.watches.lock().unwrap();
        fake.listed += 1;
        Ok(fake.entries.clone())
    }

    async fn check_watched_replies(&self, batch: &[NewMailNotice]) -> Vec<NewMailNotice> {
        let mut fake = self.watches.lock().unwrap();
        fake.checked.push(batch.len());
        fake.replies.clone()
    }
}
