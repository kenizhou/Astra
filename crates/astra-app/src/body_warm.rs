// SPDX-License-Identifier: GPL-3.0-only
//! The warm and cap pass: after an account's sync, the in-window mail
//! missing a body is warmed through the engine's batched verb under the
//! account's byte cap, and the cap is enforced with the engine's drop-over
//! verb once the warm settles, so opens are instant for mail that has
//! synced once and the store never holds a source over the cap.
//!
//! The cap is per message, not per mailbox: the engine's drop-over verb
//! forgets individual cached sources larger than the figure, and the warm
//! applies the same figure when deciding what to pre-fetch, so the two
//! halves of the rule can never disagree. A message whose size the provider
//! never stated warms (the drop verb measures it from the blob and reclaims
//! on the same pass if it was in fact over), and an `off` account warms
//! nothing and drops everything, which is the metadata-only state.

use astra_viewmodel::SyncBodyCap;
use engine_api::{AccountId, Provider};
use futures::StreamExt;

use crate::App;

/// How many missing rows one pass warms: the work list a pass draws from
/// the store, resumable by the engine's own contract (the next pass draws
/// the next slice).
pub(crate) const WARM_WORK_LIMIT: usize = 200;

impl<P: Provider> App<P> {
    /// Warms `account`'s missing bodies through `provider` under `cap`, and
    /// enforces the cap once the warm has settled. Called after each of the
    /// account's passes; failures warm nothing and are logged, never raised,
    /// because a warm is a cache fill and the next pass retries it.
    pub(crate) async fn warm_account_bodies(
        &self,
        account: &AccountId,
        provider: &P,
        cap: SyncBodyCap,
    ) {
        match cap {
            SyncBodyCap::Off => {
                // Metadata only: nothing warms, and whatever an open cached
                // goes, so the account's offline copy is its list alone.
                self.enforce_cap(account, 0).await;
            }
            SyncBodyCap::Everything => {
                self.warm_missing(account, provider, u64::MAX).await;
            }
            SyncBodyCap::Bytes { octets } => {
                self.warm_missing(account, provider, octets).await;
                self.enforce_cap(account, octets).await;
            }
        }
    }

    /// Warms the account's newest missing rows whose stated size fits
    /// `octets` (`u64::MAX` admitting every size, the everything state).
    async fn warm_missing(&self, account: &AccountId, provider: &P, octets: u64) {
        let missing = match self
            .engine
            .mail_missing_body(std::slice::from_ref(account), WARM_WORK_LIMIT)
            .await
        {
            Ok(missing) => missing,
            Err(err) => {
                log::debug!("warm: the missing-body read refused: {err}");
                return;
            }
        };
        if missing.is_empty() {
            return;
        }
        // The size filter runs on the row, before the messages are even
        // resolved: a message the provider says is over the cap never
        // reaches the fetch. A row with no stated size admits, and the drop
        // verb measures the truth from the blob and reclaims if it was.
        let keys: Vec<_> = missing
            .iter()
            .filter(|row| row.mail.size_octets.is_none_or(|size| size <= octets))
            .map(|row| row.mail.key.clone())
            .collect();
        let messages = match self.engine.messages_by_keys(account, &keys).await {
            Ok(messages) => messages,
            Err(err) => {
                log::debug!("warm: the work list could not be resolved: {err}");
                return;
            }
        };
        if messages.is_empty() {
            return;
        }
        let engine = std::sync::Arc::clone(&self.engine);
        let mut warmed = 0usize;
        let mut stream = Box::pin(engine.warm_message_sources(provider, account, &messages));
        while let Some((_index, outcome)) = stream.next().await {
            match outcome {
                Ok(()) => warmed += 1,
                Err(err) => log::debug!("warm: one of the work list refused: {err}"),
            }
        }
        if warmed > 0 {
            log::debug!("warm: {warmed} source(s) cached");
        }
    }

    /// Enforces the per-message cap on what the account already holds: the
    /// engine's drop-over verb, which leaves every message and its
    /// extracted text standing and forgets only the oversized raw sources.
    async fn enforce_cap(&self, account: &AccountId, octets: u64) {
        match self.engine.drop_message_sources_over(account, octets).await {
            Ok(dropped) if dropped.sources_removed > 0 => {
                log::debug!(
                    "warm: {} source(s) over the cap dropped",
                    dropped.sources_removed
                );
            }
            Ok(_) => {}
            Err(err) => log::debug!("warm: the cap could not be enforced: {err}"),
        }
    }
}

#[cfg(test)]
#[path = "body_warm_tests.rs"]
mod tests;
