// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox list's row cache: one load of every account's folder tree, reused by
//! every rebuild it answers. A small value type the snapshot path reads through; the
//! load, the generation check, and the invalidation live in `snapshot`, beside the
//! rebuild that pays for them.

use std::sync::Arc;

use engine_api::{AccountId, Mailbox};

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
