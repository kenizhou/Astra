// SPDX-License-Identifier: GPL-3.0-only
//! The showcase's [`WatchPort`] impl: an honest empty for the listing and the
//! check, and a static no-op for the writes, the same shape the showcase's
//! other write verbs answer (a screenshot run needs states, not transitions).
//! Its own child of the app module, a distinct trait from `AppPort`.

use astra_app::{NewMailNotice, RowRef};
use astra_watches::WatchEntry;
use async_trait::async_trait;

use crate::app_state::WatchPort;

use super::ShowcaseApp;

#[async_trait]
impl WatchPort for ShowcaseApp {
    async fn watch_thread(&self, _row: RowRef) -> Result<bool, String> {
        Ok(false)
    }

    async fn unwatch_thread(&self, _row: RowRef) -> Result<bool, String> {
        Ok(false)
    }

    async fn list_watches(&self) -> Result<Vec<WatchEntry>, String> {
        Ok(Vec::new())
    }

    async fn check_watched_replies(&self, _batch: &[NewMailNotice]) -> Vec<NewMailNotice> {
        Vec::new()
    }
}
