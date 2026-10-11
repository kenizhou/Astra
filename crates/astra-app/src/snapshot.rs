// SPDX-License-Identifier: GPL-3.0-only
//! Mailbox-list snapshot building: project the accounts in view into the
//! [`MailboxListSnapshot`] the host renders, and signal [`Surface::MailboxList`]. An
//! `impl App` block reusing the runtime's fields.
//!
//! The pane projection is one walk: every configured account's whole tree, read through
//! the row cache, sorted by the shared view-model, truncated to the visible window, and
//! stamped with the persisted expansion at publish time. The message list is the
//! selected folder's slice of one account's windowed read (plus, threaded, the thread
//! index's completion of the conversations it touched), projected by `list_rows`.

use std::{
    collections::{BTreeSet, HashSet},
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

use astra_viewmodel::{sorted_folder_rows, AccountFolderRow, MailboxListSnapshot};
use engine_api::{AccountId, MailListRow, Mailbox, Provider, ThreadId};

use crate::{
    list_rows, reference::FolderRef, Account, App, CachedMailRows, CachedRows, Scope, ViewMode,
};

impl<P: Provider> App<P> {
    /// The mailbox-list snapshot, for the host's pull after a
    /// [`Surface::MailboxList`](crate::Surface::MailboxList) signal.
    #[must_use]
    pub fn mailbox_list(&self) -> MailboxListSnapshot {
        self.mailbox_list.get()
    }

    /// The sync-progress snapshot, for the host's pull after a
    /// [`Surface::SyncProgress`](crate::Surface::SyncProgress) signal: what every mail
    /// pass has folded into the shared cell so far.
    #[must_use]
    pub fn sync_progress(&self) -> astra_viewmodel::SyncProgressSnapshot {
        self.sync_progress.snapshot()
    }

    /// Reads the accounts in view and projects them into the mailbox-list snapshot;
    /// then signals [`Surface::MailboxList`](crate::Surface::MailboxList).
    pub(crate) async fn rebuild_snapshot(&self) {
        // The generation is captured here rather than deeper in: this rebuild answers
        // the list as it stands now, and `rebuild_snapshot_for` re-checks the
        // generation before publishing; see [`bump_generation`](Self::bump_generation)
        // for what that defends against.
        let generation = self.generation.load(Ordering::SeqCst);
        self.rebuild_snapshot_for(generation).await;
    }

    /// [`rebuild_snapshot`](Self::rebuild_snapshot) for one captured generation: the
    /// list is built, then published **only** if no newer intent has landed while it
    /// was being built.
    ///
    /// The generation is a parameter rather than a read at the top because it is what
    /// an in-flight rebuild carries, and handing a test the pair an *older* one
    /// carried reproduces the race exactly, without having to win a scheduling
    /// coin-toss for it.
    pub(crate) async fn rebuild_snapshot_for(&self, generation: u64) {
        let start = Instant::now();
        let handles = self.account_handles().await;
        let ids: Vec<AccountId> = handles.iter().map(|account| account.id.clone()).collect();
        let limit = self.visible_limit();
        // One read of the scope and the mode: the folder and the grouping it is drawn
        // in are one value each, so a snapshot can never pair one account's folder with
        // another's grouping.
        let scope = self.scope.lock().expect("scope mutex poisoned").clone();
        let mode = self.view_mode();

        let mut snapshot = self
            .mailbox_snapshot(&handles, &ids, &scope, mode, limit)
            .await;
        self.restamp_expansion(&mut snapshot);
        let accounts = snapshot.account_folders.len();
        let rows = snapshot.rows.len();
        if self.generation.load(Ordering::SeqCst) != generation {
            log::info!(
                "rebuild_snapshot: {accounts} account(s) superseded by a newer intent after \
                 {}ms, dropped",
                start.elapsed().as_millis(),
            );
            return;
        }
        self.mailbox_list.publish(snapshot);
        log::info!(
            "rebuild_snapshot: {accounts} account(s), {rows} row(s) in {}ms",
            start.elapsed().as_millis(),
        );
    }

    /// The mailbox list for the current window: every account's tree, read through the
    /// row cache, truncated across the flattened pane rows so only the visible window
    /// is built and crossed the IPC. When the scope is one folder, the same window reads
    /// that folder's account and projects its newest rows into the list.
    async fn mailbox_snapshot(
        &self,
        handles: &[Arc<Account<P>>],
        ids: &[AccountId],
        scope: &Scope,
        mode: ViewMode,
        limit: usize,
    ) -> MailboxListSnapshot {
        let trees = self.load_rows(ids).await;
        let mut account_folders = Vec::with_capacity(handles.len());
        // How many rows the window still holds, spent account by account in pane
        // order: the first accounts fill the page and the later ones wait for a
        // ShowMore, which is the same contract the merged message list follows.
        let mut budget = limit;
        for (account, tree) in handles.iter().zip(trees.iter()) {
            let mut folders = sorted_folder_rows(tree);
            if folders.len() > budget {
                folders.truncate(budget);
            }
            budget = budget.saturating_sub(folders.len());
            account_folders.push(AccountFolderRow {
                account_id: account.id.clone(),
                display_name: account.identity.email.clone(),
                // Open until the publisher stamps the persisted state over it; see
                // `AccountFolderRow::expanded`.
                expanded: true,
                folders,
            });
        }
        // The list fields stay at rest outside a folder view: a whole-account or
        // unified list is a later slice, and a snapshot built for the pane alone crosses
        // with exactly the shape it always had.
        let Scope::Folder(folder) = scope else {
            return MailboxListSnapshot {
                account_folders,
                ..Default::default()
            };
        };
        let rows = self.folder_rows(folder, mode, limit).await;
        MailboxListSnapshot {
            account_folders,
            rows,
            // The selection is Task 3's to drive; nothing selects a row from inside the
            // core yet, and `None` is the field's at-rest value.
            selection: None,
            visible_limit: u32::try_from(limit).unwrap_or(u32::MAX),
            view_mode: mode,
            search: None,
        }
    }

    /// The selected folder's message rows: the account's windowed rows, completed (in
    /// the threaded view) with the conversations' out-of-window members, projected into
    /// `mode` and truncated to the visible window.
    async fn folder_rows(
        &self,
        folder: &FolderRef,
        mode: ViewMode,
        limit: usize,
    ) -> Vec<astra_viewmodel::MessageRowView> {
        let window = self.load_window();
        let base = self.load_mail_rows(&folder.account, window).await;
        let rows = self.complete_threads(&folder.account, mode, base).await;
        list_rows::project(mode, &rows, &folder.key, limit)
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
    /// needs no completion.
    async fn complete_threads(
        &self,
        account: &AccountId,
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
            .mail_on_threads(std::slice::from_ref(account), threads)
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

    /// Re-reads each account's folder-tree expansion **at publish time**, overwriting
    /// whatever the projection captured when it began.
    ///
    /// A rebuild spans several `await`s (store reads per account), so one that started
    /// before the user's chevron finishes after it and would publish the expansion as
    /// it was *then*; springing the tree back open a beat after they shut it. The same
    /// shape as the generation counter: the pass that started earlier must not win by
    /// finishing later. One lock for the whole pane rather than one per row.
    fn restamp_expansion(&self, snapshot: &mut MailboxListSnapshot) {
        let pane = self.pane.lock().expect("preferences mutex poisoned");
        for account in &mut snapshot.account_folders {
            account.expanded = pane.account_expanded(account.account_id.as_str());
            pane.stamp(account.account_id.as_str(), &mut account.folders);
        }
    }

    /// The trees for the accounts in view, from the in-memory cache; one store read per
    /// account on first use, reused for every later rebuild of the same list.
    ///
    /// A cached load is reused when it spans exactly these accounts; a different set
    /// (an account added or removed) reloads.
    /// [`invalidate_list_cache`](Self::invalidate_list_cache) drops it after a sync so
    /// it never serves stale rows.
    async fn load_rows(&self, accounts: &[AccountId]) -> Arc<Vec<Arc<Vec<Mailbox>>>> {
        if let Some(cached) = self
            .row_cache
            .lock()
            .expect("row-cache mutex poisoned")
            .as_ref()
            .filter(|cached| cached.serves(accounts))
        {
            return Arc::clone(&cached.rows);
        }
        // Miss: snapshot the cache generation, then read from the store without holding
        // the lock across the await.
        let generation = self.row_cache_generation.load(Ordering::SeqCst);
        let load_start = Instant::now();
        let rows: Arc<Vec<Arc<Vec<Mailbox>>>> = Arc::new(
            futures::future::join_all(accounts.iter().map(|id| self.engine.mailboxes(id)))
                .await
                .into_iter()
                .map(|read| Arc::new(read.unwrap_or_default()))
                .collect(),
        );
        log::debug!(
            "list rows: read {} account tree(s) in {}ms (cache miss)",
            rows.len(),
            load_start.elapsed().as_millis(),
        );
        // Only publish this load if no sync invalidated the cache while it was in
        // flight; otherwise a slow pre-sync read could land *after* the invalidation
        // and resurrect stale rows. On a mismatch, return the freshly-read rows for
        // this one snapshot without caching them; the sync that bumped the generation
        // rebuilds from its own fresh read.
        if self.row_cache_generation.load(Ordering::SeqCst) == generation {
            *self.row_cache.lock().expect("row-cache mutex poisoned") = Some(CachedRows {
                accounts: accounts.to_vec(),
                rows: Arc::clone(&rows),
            });
        }
        rows
    }

    /// Drops both cached lists so the next snapshot re-reads them; called after a sync
    /// commits new state, and after a write the store already shows. The read it forces
    /// is one store query per list, which is why this is not per-account.
    pub(crate) fn invalidate_list_cache(&self) {
        self.row_cache_generation.fetch_add(1, Ordering::SeqCst);
        *self.row_cache.lock().expect("row-cache mutex poisoned") = None;
        *self
            .mail_row_cache
            .lock()
            .expect("mail-row-cache mutex poisoned") = None;
    }

    /// Whether the shown list's rows are currently held in memory.
    #[cfg(test)]
    pub(crate) fn list_cache_is_loaded(&self) -> bool {
        self.row_cache
            .lock()
            .expect("row-cache mutex poisoned")
            .is_some()
    }

    /// The list generation: bumped by every intent that changes what the list shows,
    /// so an in-flight rebuild for the older list cannot win by finishing last.
    pub(crate) fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// The current list generation, for [`rebuild_snapshot_for`](Self::rebuild_snapshot_for).
    #[cfg(test)]
    pub(crate) fn list_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// The row-cache generation, for the stale-load guard in [`load_rows`](Self::load_rows).
    #[cfg(test)]
    pub(crate) fn row_cache_generation(&self) -> u64 {
        self.row_cache_generation.load(Ordering::SeqCst)
    }

    /// Opens or shuts one account's folder tree, and persists the choice.
    ///
    /// Deliberately touches neither the selected account nor the selected folder:
    /// expanding is not navigating. That separation is what stops the tree collapsing
    /// when the user moves to the unified view.
    pub(crate) async fn set_account_expanded(&self, account: &AccountId, expanded: bool) {
        let changed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_account_expanded(account.as_str(), expanded);
        if changed {
            self.rebuild_snapshot().await;
        }
    }

    /// Opens or shuts the folders inside one folder, and persists the choice. An
    /// account's tree and a folder's are the same thing one level down, so this follows
    /// [`set_account_expanded`](Self::set_account_expanded)'s rules exactly.
    pub(crate) async fn set_folder_expanded(&self, folder: &FolderRef, expanded: bool) {
        let changed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_folder_expanded(folder.account.as_str(), &folder.key, expanded);
        if changed {
            self.rebuild_snapshot().await;
        }
    }

    /// The current mailbox-list grouping, seeded from the persisted preference.
    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.pane
            .lock()
            .expect("preferences mutex poisoned")
            .view_mode()
    }

    /// Records the selection and persists the selected folder (or its clearing).
    pub(crate) fn set_scope(&self, scope: Scope) {
        self.pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_selected_folder(match &scope {
                Scope::Folder(folder) => Some(folder.clone()),
                Scope::AllInboxes | Scope::Account(_) => None,
            });
        *self.scope.lock().expect("scope mutex poisoned") = scope;
    }

    /// The selected folder, or `None` on a whole-account or unified view; for tests
    /// asserting what a fresh app seeded from the preferences file.
    #[cfg(test)]
    pub(crate) fn scope_for_test(&self) -> Option<FolderRef> {
        match &*self.scope.lock().expect("scope mutex poisoned") {
            Scope::Folder(folder) => Some(folder.clone()),
            Scope::AllInboxes | Scope::Account(_) => None,
        }
    }
}
