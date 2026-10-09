// SPDX-License-Identifier: GPL-3.0-only
//! Background mail-sync orchestration: the app methods that reconcile and sync whole
//! accounts, and the window every pass syncs within. The per-account pass itself lives
//! in `sync_account`; the outbox drain a pass ends with lives in `outbox`; the fan-out
//! here is what [`Intent::RefreshMail`](crate::Intent::RefreshMail) drives.

use std::time::Instant;

use engine_api::{AccountId, CalendarDate, Provider, StreamTuning, SyncWindow};
use futures::future::join_all;

use crate::{accounts::NEW_ACCOUNT_SYNC_DEPTH_MONTHS, sync_account::sync_account_providers, App};

/// Objects a streamed pass asks the provider for per network round trip.
pub(crate) const STREAM_FETCH_BATCH: usize = 200;

/// Objects committed per chunk, and so the granularity a pass costs, not just reports.
///
/// A maximum, not a target: a push delivering two messages still commits once. Its
/// ceiling is how coarsely a cold list may fill in: a chunk at a time, each a fraction
/// of the [`STREAM_FETCH_BATCH`] the network already made the user wait for.
pub(crate) const STREAM_CHUNK_SIZE: usize = 50;

impl<P: Provider> App<P> {
    /// Reconciles + syncs every account **concurrently**, drains whatever the outbox
    /// holds, then rebuilds + signals the snapshot. Routine refreshes keep cursors
    /// intact so QRESYNC / resume can do cheap incremental work.
    pub(crate) async fn refresh_mail(&self) {
        // Clone the account handles, then sync over them with the read guard released;
        // the per-folder syncs are network round-trips and must not hold the lock (a
        // long round-trip would stall a concurrent account add).
        let accounts = self.account_handles().await;
        log::info!("refresh_mail: syncing {} account(s)", accounts.len());
        let sync_start = Instant::now();
        join_all(accounts.iter().enumerate().map(|(at, account)| {
            let tuning = self.sync_tuning();
            async move {
                sync_account_providers(
                    &self.engine,
                    account,
                    self.connector.as_deref(),
                    tuning,
                    &self.sync_progress,
                    at,
                )
                .await;
            }
        }))
        .await;
        // The pass committed new state, so the cached load is stale from here on.
        self.invalidate_list_cache();
        // Anything queued whose backoff has elapsed goes out on the same pass; a send
        // the last run left behind should not wait for the next one.
        self.drain_outboxes().await;
        let sync_ms = sync_start.elapsed().as_millis();
        let rebuild_start = Instant::now();
        self.rebuild_snapshot().await;
        log::info!(
            "refresh_mail: sync {sync_ms}ms + rebuild {}ms",
            rebuild_start.elapsed().as_millis(),
        );
    }

    /// Syncs one account's mail (used when it is first added), without touching the
    /// others. The caller rebuilds afterwards for the authoritative snapshot.
    pub(crate) async fn sync_account(&self, id: &AccountId) {
        let Some(account) = self.account_handle(id).await else {
            return;
        };
        let tuning = self.sync_tuning();
        let at = self.account_ordinal(id).await;
        sync_account_providers(
            &self.engine,
            &account,
            self.connector.as_deref(),
            tuning,
            &self.sync_progress,
            at,
        )
        .await;
        self.invalidate_list_cache();
    }

    /// The tuning every pass runs with: the fetch and chunk sizes above, bounded by the
    /// product default sync-depth window.
    // A method rather than an associated function: the depth is per-account in the
    // engine's contract, and this is where a per-account choice will be read from.
    #[allow(clippy::unused_self)]
    pub(crate) fn sync_tuning(&self) -> StreamTuning {
        StreamTuning::new(STREAM_FETCH_BATCH, STREAM_CHUNK_SIZE)
            .within(sync_window(NEW_ACCOUNT_SYNC_DEPTH_MONTHS))
    }

    /// Puts back what the previous run left unfinished in the outbox.
    ///
    /// Called once at start-up, before any drain or write runs. Without this the next
    /// run would still recover them, but only once their lease lapsed, minutes later.
    pub(crate) async fn recover_outbox(&self) {
        match self.engine.recover_interrupted_ops().await {
            Ok(0) => {}
            Ok(count) => log::warn!(
                "outbox: {count} queued write(s) were cut off when the app last stopped, and \
                 were put back"
            ),
            Err(err) => log::error!(
                "outbox: could not recover the writes the app left unfinished when it last \
                 stopped; they are retried once their attempt expires: {err}"
            ),
        }
    }
}

/// The window a pass of `months` depth syncs within: mail delivered on or after the
/// day `months` calendar months ago, or the whole mailbox at `0`.
///
/// Subtracts whole calendar months (not 30-day spans), clamping the day to the target
/// month's length: Mar 31 minus one month is Feb 28.
pub(crate) fn sync_window(months: u16) -> SyncWindow {
    let today = time::OffsetDateTime::now_utc().date();
    let Some(cutoff) = cutoff(today, i32::from(months)) else {
        return SyncWindow::full();
    };
    match CalendarDate::new(cutoff.year(), u8::from(cutoff.month()), cutoff.day()) {
        Ok(date) => SyncWindow::since(date),
        Err(_) => SyncWindow::full(),
    }
}

/// The date `months` before `today`, or `None` for zero (no cutoff).
fn cutoff(today: time::Date, months: i32) -> Option<time::Date> {
    if months == 0 {
        return None;
    }
    // Work in absolute months since year 0 so the subtraction borrows across years.
    let month_index = i32::from(u8::from(today.month())) - 1; // 0..=11
    let total = today.year() * 12 + month_index - months;
    let year = total.div_euclid(12);
    let month = u8::try_from(total.rem_euclid(12) + 1)
        .ok()
        .and_then(|value| time::Month::try_from(value).ok())?;
    let day = today.day().min(month.length(year));
    time::Date::from_calendar_date(year, month, day).ok()
}
