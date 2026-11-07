// SPDX-License-Identifier: GPL-3.0-only
//! The calendar cadence: the per-account sync half every calendar fetch
//! shares, and the change-gated refresh the host's periodic timer calls.
//! Both are bodies rather than intents because a fetch that changed nothing
//! must signal nothing: the boot's join fills the store silently (the prime
//! already painted the persisted diary, and the pane's own open refresh is
//! what republishes it), and a standing tick republishes only when the
//! diary moved.

use std::sync::Arc;

use engine_api::{AccountId, Provider};

use crate::{calendar::calendar_provider_of, Account, App};

impl<P: Provider> App<P> {
    /// Fetches one account's calendar into the store, silently: the refresh
    /// body's per-account half without the closing rebuild, so no calendar
    /// surface is signalled. The boot's first-sync join and every
    /// account-add flow that spawns it call this; the diary the user sees
    /// is republished by the pane's own open refresh, over the filled
    /// store.
    pub(crate) async fn fetch_account_calendar(&self, id: &AccountId) {
        let Some(handle) = self.account_handle(id).await else {
            return;
        };
        self.sync_account_calendar(&handle).await;
    }

    /// Syncs every account's calendars and republishes the diary only when
    /// it moved: the standing cadence's body, whose surface signal fires on
    /// change alone. The now line rides the published copy, never the
    /// comparison, because a clock advancing is not a change.
    pub async fn refresh_calendars_if_changed(&self) {
        for handle in self.account_handles().await {
            self.sync_account_calendar(&handle).await;
        }
        let fresh = self.build_calendar_snapshot().await;
        if diary_moved(&self.calendar.get(), &fresh) {
            self.calendar.publish(fresh);
        }
    }

    /// Syncs one account's calendars over the standing horizon: the
    /// per-account half every calendar refresh rides. A successful re-read
    /// clears the account's write state (the recovery a `FailedWithRetry`
    /// row waits for); a busy or failed pass logs and steps aside, because
    /// the next cadence is the retry.
    pub(crate) async fn sync_account_calendar(&self, handle: &Arc<Account<P>>) {
        let Some(provider) = calendar_provider_of(handle) else {
            return;
        };
        let zone = self.calendar_inputs().zone;
        let horizon = self.sync_horizon();
        let at = self.account_ordinal(&handle.id).await;
        match self
            .engine
            .sync_calendar(provider, &handle.id, horizon, &zone)
            .await
        {
            Ok(_) => {
                self.clear_calendar_write(&handle.id);
                log::info!("calendar[a{at}]: the calendars synced");
            }
            Err(engine_api::ApiError::Busy) => {
                log::info!("calendar[a{at}]: another sync holds the calendar; skipped");
            }
            Err(err) => log::warn!("calendar[a{at}]: the calendar sync failed: {err}"),
        }
    }
}

/// Whether the diary moved between two snapshots: every field but the now
/// line, which advances with every rebuild and is never a change.
fn diary_moved(
    before: &astra_viewmodel::CalendarSnapshot,
    after: &astra_viewmodel::CalendarSnapshot,
) -> bool {
    before.calendars != after.calendars
        || before.occurrences != after.occurrences
        || before.write_states != after.write_states
        || before.reply_prompts != after.reply_prompts
        || before.shape != after.shape
}

#[cfg(test)]
#[path = "../calendar_cadence_tests.rs"]
mod tests;
