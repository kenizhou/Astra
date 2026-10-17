// SPDX-License-Identifier: GPL-3.0-only
//! The message-list row loads behind the snapshot: the selected folder's
//! windowed read, the unified scope's merged read, and the threaded view's
//! completion of the conversations the window touches. Split from `snapshot`
//! so each file stays under the line cap; an `impl App` block over the same
//! runtime fields.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

use engine_api::{AccountId, MailListRow, Mailbox, MailboxRole, Provider, ThreadId};

use crate::{list_rows, reference::FolderRef, row_cache::CachedMailRows, App, ViewMode};

impl<P: Provider> App<P> {
    /// The selected folder's message rows: the account's windowed rows, completed (in
    /// the threaded view) with the conversations' out-of-window members, projected into
    /// `mode` and truncated to the visible window.
    pub(crate) async fn folder_rows(
        &self,
        folder: &FolderRef,
        mode: ViewMode,
        limit: usize,
    ) -> Vec<astra_viewmodel::MessageRowView> {
        let window = self.load_window();
        let base = self.load_mail_rows(&folder.account, window).await;
        let rows = self
            .complete_threads(std::slice::from_ref(&folder.account), mode, base)
            .await;
        list_rows::project(mode, &rows, &folder.key, limit)
    }

    /// Every account's inbox rows, merged newest first: the unified list. The
    /// windowed read spans every account in one store query, and the
    /// projection is the same row shape the folder view projects, so a row the
    /// unified list renders acts through its own account exactly as it does in
    /// its folder.
    pub(crate) async fn unified_rows(
        &self,
        ids: &[AccountId],
        trees: &[Arc<Vec<Mailbox>>],
        mode: ViewMode,
        limit: usize,
    ) -> Vec<astra_viewmodel::MessageRowView> {
        let window = self.load_window();
        let base: Arc<Vec<Arc<MailListRow>>> = Arc::new(
            self.engine
                .mail_window(ids, window)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(Arc::new)
                .collect(),
        );
        let rows = self.complete_threads(ids, mode, base).await;
        // Each account's inbox is its own key: an account whose tree holds no
        // role-bearing Inbox contributes no rows, never another account's.
        let inboxes: BTreeMap<String, String> = ids
            .iter()
            .zip(trees.iter())
            .filter_map(|(id, tree)| {
                tree.iter()
                    .find(|mailbox| mailbox.role.as_ref() == Some(&MailboxRole::Inbox))
                    .map(|inbox| (id.as_str().to_owned(), inbox.id.as_str().to_owned()))
            })
            .collect();
        list_rows::unified(mode, &rows, &inboxes, limit)
    }

    /// The newest `window` rows of `account`, from the in-memory cache; one ordered
    /// store read on first use, reused for every later rebuild of the same list at the
    /// same depth or shallower.
    ///
    /// A cached load is reused when it spans exactly this account at a **deeper or
    /// equal** window (a wider window is a superset; the projection truncates); a
    /// different account, or a deeper [`Intent::ShowMore`](crate::Intent::ShowMore)
    /// than the cache holds, reloads.
    /// [`invalidate_list_cache`](Self::invalidate_list_cache) drops it after a sync or
    /// a write, so it never serves stale rows.
    async fn load_mail_rows(
        &self,
        account: &AccountId,
        window: usize,
    ) -> Arc<Vec<Arc<MailListRow>>> {
        if let Some(cached) = self
            .mail_row_cache
            .lock()
            .expect("mail-row-cache mutex poisoned")
            .as_ref()
            .filter(|cached| cached.serves(account, window))
        {
            return Arc::clone(&cached.rows);
        }
        // Miss: snapshot the cache generation, then read from the store without holding
        // the lock across the await. Each row is wrapped in its own `Arc` so a rebuild
        // re-projects it without deep-copying it.
        let generation = self.row_cache_generation.load(Ordering::SeqCst);
        let load_start = Instant::now();
        let rows: Arc<Vec<Arc<MailListRow>>> = Arc::new(
            self.engine
                .mail_window(std::slice::from_ref(account), window)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(Arc::new)
                .collect(),
        );
        log::debug!(
            "list rows: read {} mail row(s) at window {window} in {}ms (cache miss)",
            rows.len(),
            load_start.elapsed().as_millis(),
        );
        // If the store returned fewer rows than the window asked for, this is everything
        // the account holds; cache it as satisfying an unbounded window, so scrolling
        // deeper reuses it instead of re-reading the same full set every step.
        let effective_window = if rows.len() < window {
            usize::MAX
        } else {
            window
        };
        // Only publish this load if nothing invalidated the cache while it was in
        // flight; otherwise a slow pre-sync read could land after the invalidation and
        // resurrect stale rows. On a mismatch, return the freshly-read rows for this
        // one snapshot without caching them.
        if self.row_cache_generation.load(Ordering::SeqCst) == generation {
            *self
                .mail_row_cache
                .lock()
                .expect("mail-row-cache mutex poisoned") = Some(CachedMailRows {
                account: account.clone(),
                window: effective_window,
                rows: Arc::clone(&rows),
            });
        }
        rows
    }

    /// Completes the windowed read for the threaded view: every *other* member of the
    /// conversations the window touches, pulled from the store's thread index in one
    /// indexed read, so a thread row counts its whole history even when older members
    /// fall outside the window. The flat view lists messages, not conversations, and
    /// needs no completion. Spans every account a unified list covers as one read.
    async fn complete_threads(
        &self,
        accounts: &[AccountId],
        mode: ViewMode,
        base: Arc<Vec<Arc<MailListRow>>>,
    ) -> Vec<Arc<MailListRow>> {
        if mode != ViewMode::Threaded {
            return base.to_vec();
        }
        let threads: BTreeSet<&str> = base
            .iter()
            .filter_map(|row| row.mail.thread_id.as_ref().map(ThreadId::as_str))
            .collect();
        if threads.is_empty() {
            return base.to_vec();
        }
        // Keyed on `(account, key)`: a provider key is unique within an account, not
        // across the several a unified list spans.
        let present: HashSet<(&str, &str)> = base
            .iter()
            .map(|row| (row.account.as_str(), row.mail.key.as_str()))
            .collect();
        let extra = self
            .engine
            .mail_on_threads(accounts, threads)
            .await
            .unwrap_or_default();
        let mut rows = base.to_vec();
        rows.extend(
            extra
                .into_iter()
                .filter(|member| {
                    !present.contains(&(member.account.as_str(), member.mail.key.as_str()))
                })
                .map(Arc::new),
        );
        rows
    }
}
