// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox-list view-model: an immutable snapshot a surface renders.
//!
//! Pure projection over the engine's account and mailbox state; state lives in the
//! engine, the surface renders the snapshot. Grouping and ordering happen here so every
//! surface shares one definition of what is on screen. This file holds the snapshot
//! type; the folder-row projection itself lives in `folders`.

use serde::{Deserialize, Serialize};

use crate::folders::AccountFolderRow;

/// An immutable mailbox-list snapshot for a surface to render: every account's folder
/// tree, always populated, so the pane is available whatever else the view is showing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxListSnapshot {
    /// Every account's sorted folder list, for the folder pane, one entry per configured
    /// account.
    ///
    /// This is what a folder pane renders: the whole tree stays on screen while one
    /// account is selected, so selecting an account never empties the others.
    pub account_folders: Vec<AccountFolderRow>,
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
