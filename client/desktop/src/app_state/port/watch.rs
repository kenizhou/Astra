// SPDX-License-Identifier: GPL-3.0-only
//! The core's [`WatchPort`] impl: the watch-replies leg of the app bridge, its
//! own child of the port module (a distinct trait from `AppPort`, so its impl
//! splits freely). Each verb hands through to the core's own body and maps the
//! typed refusal to the display sentence the webview surfaces.

use astra_app::{App, NewMailNotice, RowRef};
use astra_watches::WatchEntry;
use async_trait::async_trait;

use super::super::{HostProvider, WatchPort};

#[async_trait]
impl WatchPort for App<HostProvider> {
    async fn watch_thread(&self, row: RowRef) -> Result<bool, String> {
        App::watch_thread(self, row)
            .await
            .map_err(|err| err.to_string())
    }

    async fn unwatch_thread(&self, row: RowRef) -> Result<bool, String> {
        App::unwatch_thread(self, row)
            .await
            .map_err(|err| err.to_string())
    }

    async fn list_watches(&self) -> Result<Vec<WatchEntry>, String> {
        App::list_watches(self).map_err(|err| err.to_string())
    }

    async fn check_watched_replies(&self, batch: &[NewMailNotice]) -> Vec<NewMailNotice> {
        App::check_watched_replies(self, batch).await
    }
}
