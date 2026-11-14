// SPDX-License-Identifier: GPL-3.0-only
//! The watch-replies leg of the app port: a supertrait of [`super::AppPort`]
//! so each impl's watch block lives in its own child file (one trait's impl
//! cannot split across modules, distinct traits impl freely). The verbs
//! mirror the core's own; the host maps the core's typed refusal to a display
//! sentence. A local membership mark: none of these writes the engine.

use astra_app::{NewMailNotice, RowRef};
use astra_watches::WatchEntry;
use async_trait::async_trait;

/// The watch-replies verbs the command surface and the delivery hook drive.
#[async_trait]
pub(crate) trait WatchPort: Send + Sync {
    /// Watches the row's conversation, answering whether this call inserted
    /// (idempotent).
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, or a conversation that no
    /// longer resolves.
    async fn watch_thread(&self, row: RowRef) -> Result<bool, String>;

    /// Unwatches the row's conversation, answering whether a row was removed
    /// (idempotent).
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, or a message row that no
    /// longer resolves.
    async fn unwatch_thread(&self, row: RowRef) -> Result<bool, String>;

    /// Every watch, insertion order; an app with no store answers empty.
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words when the read fails.
    async fn list_watches(&self) -> Result<Vec<WatchEntry>, String>;

    /// The post-pass reply check over a drained batch's notices: the notices
    /// for the watched conversations the batch replies to, each watch
    /// consumed. The delivery hook drives it after the drain.
    async fn check_watched_replies(&self, batch: &[NewMailNotice]) -> Vec<NewMailNotice>;
}
