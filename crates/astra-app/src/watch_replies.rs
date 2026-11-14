// SPDX-License-Identifier: GPL-3.0-only
//! The core watch-replies verbs and the post-pass reply check: watch a
//! conversation, and the first reply raises a notice and clears the watch
//! (one-shot). A LOCAL membership mark, so nothing here writes the engine:
//! the marks live in the device-local [`WatchStore`] the host installs, and
//! the check resolves threads through engine READS only.
//!
//! The crux the check rests on: a derived thread is named the smallest owned
//! Message-ID in its merged component and RE-KEYS when a reply arrives with a
//! smaller id, so a watch keyed on the thread id captured at set time would
//! miss roughly half of first replies. Each watch therefore carries one
//! representative member key, and the check resolves BOTH sides through the
//! engine at check time (the batch's notice keys and every watch's member
//! key, one `mail_by_keys` per account), matching resolved-against-resolved.
//! The member row survives re-keying (its thread id moves with the
//! component), so both sides always name the current truth.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use astra_watches::{NewWatch, WatchEntry, WatchStore};
use engine_api::{AccountId, Provider, ProviderKey};
use thiserror::Error;

use crate::{list_rows, notifications::NewMailNotice, reference::RowRef, App};

/// Why a watch verb could not be applied. The typed form lets the host
/// report a readable refusal, never a raw constraint string.
#[derive(Debug, Error)]
pub enum WatchError {
    /// No watch store is installed; a write cannot proceed.
    #[error("the watch store is not installed")]
    NoStore,
    /// The conversation names nothing in the account's store.
    #[error("the conversation no longer resolves in the store")]
    Unresolved,
    /// The watch store failed.
    #[error("the watch store failed: {0}")]
    Store(#[from] astra_watches::WatchError),
}

impl<P: Provider> App<P> {
    /// Installs the watch store the core records and checks over. Call it
    /// before the first watch verb: an app with no store installed watches
    /// nothing and refuses writes, the same shape the reminder store installs.
    pub fn install_watch_store(&self, store: Arc<WatchStore>) {
        *self.watches.lock().expect("watches mutex poisoned") = Some(store);
    }

    /// The installed watch store, or `None` when the host has not installed
    /// one.
    fn watch_store(&self) -> Option<Arc<WatchStore>> {
        self.watches.lock().expect("watches mutex poisoned").clone()
    }

    /// Watches the row's conversation, capturing one representative member
    /// key, then answers whether this call inserted (idempotent). A thread
    /// row reads its newest member for the link; a message row uses its own
    /// key. NO move, NO engine write: the mark is device-local.
    ///
    /// # Errors
    ///
    /// [`WatchError::NoStore`] when no store is installed,
    /// [`WatchError::Unresolved`] when the conversation no longer resolves,
    /// else a store failure.
    pub async fn watch_thread(&self, row: RowRef) -> Result<bool, WatchError> {
        let store = self.watch_store().ok_or(WatchError::NoStore)?;
        let (account, thread_id, member_key) = self.resolve_watch_target(&row).await?;
        Ok(store.watch(&NewWatch {
            account,
            thread_id,
            member_key,
        })?)
    }

    /// Unwatches the row's conversation, answering whether a row was removed
    /// (idempotent). The one-shot consume on the first reply rides the store's
    /// same `unwatch`; this is the menu's clear leg.
    ///
    /// # Errors
    ///
    /// [`WatchError::NoStore`] when no store is installed,
    /// [`WatchError::Unresolved`] when a message row no longer resolves, else
    /// a store failure.
    pub async fn unwatch_thread(&self, row: RowRef) -> Result<bool, WatchError> {
        let store = self.watch_store().ok_or(WatchError::NoStore)?;
        let (account, thread_id) = self.resolve_watch_thread_key(&row).await?;
        Ok(store.unwatch(&account, &thread_id)?)
    }

    /// Every watch, insertion order. An app with no store answers empty.
    ///
    /// # Errors
    ///
    /// A store failure when the read cannot run.
    pub fn list_watches(&self) -> Result<Vec<WatchEntry>, WatchError> {
        let Some(store) = self.watch_store() else {
            return Ok(Vec::new());
        };
        Ok(store.list()?)
    }

    /// The post-pass reply check over a drained batch's notices: for each
    /// watch whose conversation a notice replies to, builds a notice from the
    /// first matching reply and consumes the watch (one-shot). The
    /// `watch_replies_enabled` preference gates the whole body (off answers
    /// empty and consumes nothing). Both sides resolve through the engine at
    /// check time, one `mail_by_keys` per account for the batch keys plus the
    /// watch member keys, matched resolved-against-resolved so a derived
    /// thread's re-keying never strands the watch. Fail-open: any read or
    /// store failure is logged and skips its account, never stranding the
    /// pass or the base new-mail notice.
    pub async fn check_watched_replies(&self, batch: &[NewMailNotice]) -> Vec<NewMailNotice> {
        let enabled = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .prefs
            .watch_replies_enabled;
        if !enabled || batch.is_empty() {
            return Vec::new();
        }
        let Some(store) = self.watch_store() else {
            return Vec::new();
        };
        let watches = match store.list() {
            Ok(watches) => watches,
            Err(err) => {
                log::warn!("watches: the reply check could not read the store: {err}");
                return Vec::new();
            }
        };
        if watches.is_empty() {
            return Vec::new();
        }
        let mut batch_by_account: BTreeMap<AccountId, Vec<&NewMailNotice>> = BTreeMap::new();
        for notice in batch {
            batch_by_account
                .entry(notice.account.clone())
                .or_default()
                .push(notice);
        }
        let mut watches_by_account: BTreeMap<AccountId, Vec<&WatchEntry>> = BTreeMap::new();
        for watch in &watches {
            watches_by_account
                .entry(watch.account.clone())
                .or_default()
                .push(watch);
        }

        let mut notices = Vec::new();
        // One notice per distinct reply, even when several watches match it:
        // the person hears about the reply once, and every matching watch
        // consumes.
        let mut announced: BTreeSet<(AccountId, ProviderKey)> = BTreeSet::new();
        for (account, account_watches) in watches_by_account {
            let Some(account_batch) = batch_by_account.get(&account) else {
                continue;
            };
            let mut keys: Vec<ProviderKey> = account_batch
                .iter()
                .map(|notice| notice.key.clone())
                .collect();
            keys.extend(account_watches.iter().map(|watch| watch.member_key.clone()));
            let resolved = match self.engine.mail_by_keys(&account, &keys).await {
                Ok(rows) => rows,
                Err(err) => {
                    log::warn!("watches: the reply check could not resolve an account: {err}");
                    continue;
                }
            };
            let mut thread_of: BTreeMap<ProviderKey, String> = BTreeMap::new();
            for row in &resolved {
                thread_of.insert(row.mail.key.clone(), list_rows::thread_key(row).to_owned());
            }
            for watch in account_watches {
                let Some(watch_thread) = thread_of.get(&watch.member_key) else {
                    // The representative link no longer resolves (moved or
                    // deleted): fail-open, the watch stays armed but silent.
                    continue;
                };
                let Some(reply) = account_batch.iter().find(|notice| {
                    thread_of
                        .get(&notice.key)
                        .is_some_and(|thread| thread == watch_thread)
                }) else {
                    continue;
                };
                if let Err(err) = store.unwatch(&account, &watch.thread_id) {
                    log::warn!("watches: the reply consume failed: {err}");
                }
                if announced.insert((account.clone(), reply.key.clone())) {
                    notices.push((*reply).clone());
                }
            }
        }
        notices
    }

    /// The (account, thread key) a row's watch keys: a thread row by its id
    /// directly, a message row by its source row's `thread_key` (the resolved
    /// thread id, or its own provider key when never threaded), read from the
    /// engine. A READ, so the local-only red line holds.
    async fn resolve_watch_thread_key(
        &self,
        row: &RowRef,
    ) -> Result<(AccountId, String), WatchError> {
        match row {
            RowRef::Thread(thread) => Ok((thread.account.clone(), thread.thread_id.clone())),
            RowRef::Message(message) => {
                let resolved = self
                    .engine
                    .mail_by_keys(&message.account, std::slice::from_ref(&message.key))
                    .await
                    .unwrap_or_default();
                let source = resolved.into_iter().next().ok_or(WatchError::Unresolved)?;
                Ok((
                    message.account.clone(),
                    list_rows::thread_key(&source).to_owned(),
                ))
            }
        }
    }

    /// The watch target: the thread key beside one representative member key
    /// captured at set time. A thread row reads its newest member through
    /// `mail_on_threads`; a message row uses its own key. Both are READS.
    async fn resolve_watch_target(
        &self,
        row: &RowRef,
    ) -> Result<(AccountId, String, ProviderKey), WatchError> {
        match row {
            RowRef::Thread(thread) => {
                let members = self
                    .engine
                    .mail_on_threads(
                        std::slice::from_ref(&thread.account),
                        std::iter::once(thread.thread_id.as_str()),
                    )
                    .await
                    .unwrap_or_default();
                let newest = members
                    .into_iter()
                    .max_by_key(|row| row.mail.date_utc)
                    .ok_or(WatchError::Unresolved)?;
                Ok((
                    thread.account.clone(),
                    thread.thread_id.clone(),
                    newest.mail.key,
                ))
            }
            RowRef::Message(message) => {
                let (account, thread_id) = self.resolve_watch_thread_key(row).await?;
                Ok((account, thread_id, message.key.clone()))
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_replies_tests.rs"]
mod tests;
