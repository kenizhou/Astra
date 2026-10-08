// SPDX-License-Identifier: GPL-3.0-only
//! Multi-account lifecycle: the [`Account`] value the host installs, the connector seam
//! that binds a folder on demand, and the add and first-sync paths over the shared
//! engine. The `Arc`'d account handles let a caller clone one out under the read guard
//! and drop the guard before any network `.await`, so a long provider round-trip never
//! stalls a concurrent add.

use std::sync::Arc;

use async_trait::async_trait;
use engine_api::{AccountId, EmailAddress, Provider};

use crate::App;

/// How much history a newly added account syncs, in months. The product default; a
/// first sync is the longest wait the app asks for, and the user can widen it once a
/// per-account depth setting exists.
pub(crate) const NEW_ACCOUNT_SYNC_DEPTH_MONTHS: u16 = 3;

/// One configured account the app drives, all over the shared engine.
pub struct Account<P> {
    /// The account's id (scopes everything in the shared engine store).
    pub id: AccountId,
    /// The mail providers: one per folder the account listed when it connected where
    /// mail syncs per folder (IMAP, Graph), one for the whole account otherwise (JMAP,
    /// Gmail). The host dials them and hands them here; the login blocks and the TLS
    /// trust policy and credentials live with the host.
    pub providers: Vec<P>,
    /// The from-address this account sends as; also the label its group header shows.
    pub identity: EmailAddress,
}

impl<P> core::fmt::Debug for Account<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The providers may hold sensitive connection handles; show only the identity.
        f.debug_struct("Account")
            .field("id", &self.id)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// Connects a provider bound to a single mailbox of an account.
///
/// The app is generic over `P: Provider` and cannot itself open IMAP connections:
/// login blocks, and the TLS trust policy and credentials live in the host. An account
/// binds every folder it lists when it connects, so the app asks this port for a folder
/// only when the account's list names one no provider of it is bound to (a folder
/// created since it connected, on the server or by another client). The host implements
/// it over `astra_account::connect_imap_mailbox`. `Send + Sync` so the app can call it
/// across `.await` on the host's multi-threaded runtime.
#[async_trait]
pub trait MailboxConnector<P>: Send + Sync {
    /// Connects a provider bound to `mailbox_key` of `account`. Returns `None` if it
    /// cannot (an unknown account, or a connection or login failure: the app then
    /// leaves the folder empty rather than failing the pass).
    async fn connect_folder(&self, account: &AccountId, mailbox_key: &str) -> Option<P>;
}

impl<P: Provider> App<P> {
    /// Registers a newly-added user account without syncing, then gives it the product
    /// default three-month depth before its first background sync starts: the host
    /// installs the account (so the pane shows it and the setup flow can dismiss at
    /// once), then spawns [`sync_added_account`](Self::sync_added_account) itself; the
    /// first sync is the longest wait the app asks for and must not block the install.
    pub async fn add_new_account_deferred(&self, account: Account<P>) {
        self.install_account(account).await;
        self.rebuild_snapshot().await;
    }

    /// Puts `account` into the pane; replacing an existing entry **in place**, else
    /// appending.
    ///
    /// The position is the contract. Account ids are unique here, so re-adding one (a
    /// reconnect, a credential change) replaces its providers rather than duplicating
    /// it; what it must not do is move it. Every surface renders the account list
    /// verbatim, and an interactive launch replaces *every* account as its background
    /// dial lands, three at a time, in network-latency order. Removing and re-appending
    /// therefore re-sorted the whole pane once per account on every boot, visibly, for
    /// as long as the slowest dial took.
    async fn install_account(&self, account: Account<P>) {
        let mut accounts = self.accounts.write().await;
        match accounts
            .iter()
            .position(|existing| existing.id == account.id)
        {
            Some(at) => accounts[at] = Arc::new(account),
            None => accounts.push(Arc::new(account)),
        }
    }

    /// Syncs a just-added account's mail and rebuilds the snapshot so its folders
    /// appear. The host registers the account first (via
    /// [`add_new_account_deferred`](Self::add_new_account_deferred)), then spawns this
    /// in the background: the install is instant and this is the slow half. A no-op for
    /// an unknown account id.
    pub async fn sync_added_account(&self, id: &AccountId) {
        self.sync_account(id).await;
        self.rebuild_snapshot().await;
    }

    /// Renders the mailbox-list snapshot from the **already-persisted** store, without
    /// syncing: so a host can show the cached pane the instant it boots, before the
    /// background sync runs. The store survives across launches, so on a returning
    /// user this paints the last-synced tree immediately. A first run (empty store)
    /// simply renders an empty pane.
    pub async fn prime_snapshot(&self) {
        self.rebuild_snapshot().await;
    }

    /// Every account's id, in the order the host stored them.
    pub(crate) async fn account_ids(&self) -> Vec<AccountId> {
        self.accounts
            .read()
            .await
            .iter()
            .map(|account| account.id.clone())
            .collect()
    }

    /// Cheap clones of every account handle, taken under the read guard and returned
    /// so the caller can iterate and do network I/O over them with the guard already
    /// released.
    pub(crate) async fn account_handles(&self) -> Vec<Arc<Account<P>>> {
        self.accounts.read().await.iter().map(Arc::clone).collect()
    }

    /// A cheap clone of one account's handle, taken the same way; `None` if no account
    /// has that id.
    pub(crate) async fn account_handle(&self, id: &AccountId) -> Option<Arc<Account<P>>> {
        self.accounts
            .read()
            .await
            .iter()
            .find(|account| &account.id == id)
            .map(Arc::clone)
    }

    /// The account's position in the stored list, for the diagnostic log.
    ///
    /// **Stable across passes, which a per-pass ordinal is not.** An address can never
    /// appear in a log line, which is what rules out the obvious alternative, so `a2`
    /// in the log is the third account on screen. `usize::MAX` for an account that is
    /// no longer stored: a sync finishing just after a removal, which is rare and
    /// better labelled oddly than dropped.
    pub(crate) async fn account_ordinal(&self, id: &AccountId) -> usize {
        self.accounts
            .read()
            .await
            .iter()
            .position(|account| &account.id == id)
            .unwrap_or(usize::MAX)
    }
}
