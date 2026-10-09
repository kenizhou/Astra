// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox list's row caches: one load of every account's folder tree for the pane,
//! and one windowed load of the selected folder's account for the message list, each
//! reused by every rebuild it answers. Small value types the snapshot path reads
//! through; the loads, the generation checks, and the invalidation live in `snapshot`,
//! beside the rebuild that pays for them.

use std::sync::Arc;

use engine_api::{AccountId, MailListRow, Mailbox};

/// One cached mailbox-list load: the accounts it spans, in pane order, and each one's
/// mailboxes individually `Arc`-ed, as [`App::row_cache`](crate::App) holds them.
///
/// No window dimension, unlike the message-row cache this pattern is ported from: the
/// pane reads each account's whole tree in one call, and the visible window truncates
/// the projection afterwards, so a cached load answers any window the view asks for.
pub(crate) struct CachedRows {
    /// The accounts this load spans; a read of a different set is a different load.
    pub(crate) accounts: Vec<AccountId>,
    /// Each account's whole tree, in the same order as [`accounts`](Self::accounts).
    pub(crate) rows: Arc<Vec<Arc<Vec<Mailbox>>>>,
}

impl CachedRows {
    /// Whether this load answers a read of `accounts`.
    ///
    /// The accounts must match exactly: the pane draws every configured account, so any
    /// addition or removal is a different list.
    pub(crate) fn serves(&self, accounts: &[AccountId]) -> bool {
        self.accounts == accounts
    }
}

/// One cached message-list load: the newest `window` rows of one account, each
/// individually `Arc`-ed, as [`App::mail_row_cache`](crate::App) holds them. The folder
/// filter is a projection decision, not a read dimension, so one cached account window
/// answers every folder of that account.
pub(crate) struct CachedMailRows {
    /// The account this load spans; a read of a different account is a different load.
    pub(crate) account: AccountId,
    /// How deep into the account's mail this load reaches. `usize::MAX` when the store
    /// already returned everything it holds (a read that asked for more rows than the
    /// account has), so scrolling deeper reuses the cache instead of re-reading the same
    /// full set.
    pub(crate) window: usize,
    /// The account's rows, newest first, each shared so a rebuild re-projects without
    /// deep-copying them.
    pub(crate) rows: Arc<Vec<Arc<MailListRow>>>,
}

impl CachedMailRows {
    /// Whether this load answers a read of `account` at `window`: the same account, at a
    /// deeper or equal window (a wider cached window is a superset; the projection
    /// truncates).
    pub(crate) fn serves(&self, account: &AccountId, window: usize) -> bool {
        &self.account == account && self.window >= window
    }
}
