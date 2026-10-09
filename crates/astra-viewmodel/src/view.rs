// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox-list view-model: an immutable snapshot a surface renders.
//!
//! Pure projection over the engine's account and mailbox state; state lives in
//! the engine, the surface renders the snapshot. Grouping and ordering happen
//! here so every surface shares one definition of what is on screen. This file
//! holds the snapshot type; the folder-row projection itself lives in
//! `folders`, the message-row types in `rows`.

use serde::{Deserialize, Serialize};

use crate::folders::AccountFolderRow;
use crate::rows::{MessageRowView, RowKey, ViewMode};

/// An immutable mailbox-list snapshot for a surface to render: every account's
/// folder tree, always populated, so the pane is available whatever else the
/// view is showing, and the message list the selected folder projects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxListSnapshot {
    /// Every account's sorted folder list, for the folder pane, one entry per
    /// configured account.
    ///
    /// This is what a folder pane renders: the whole tree stays on screen
    /// while one account is selected, so selecting an account never empties
    /// the others.
    pub account_folders: Vec<AccountFolderRow>,
    /// The message rows in display order, at most the visible window of them.
    ///
    /// Empty while no mail is listed: a snapshot built for the folder pane
    /// alone crosses with no rows, exactly the shape the snapshot carried
    /// before the message list joined it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<MessageRowView>,
    /// The selected row's key, or `None` with nothing selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<RowKey>,
    /// How many rows the visible window holds: the top of the list a surface
    /// is currently owed, grown by a show-more and reset by any navigation
    /// that changes the list.
    ///
    /// `0` is the at-rest value of a snapshot listing no mail; `rows` is the
    /// authority on what is on screen, this is the window it was built
    /// against.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub visible_limit: u32,
    /// The mode the rows are grouped in.
    ///
    /// Absent on the wire when flat, the default, so a snapshot listing no
    /// mail crosses with no view mode at all.
    #[serde(default, skip_serializing_if = "is_flat")]
    pub view_mode: ViewMode,
}

/// Whether a count is zero, the at-rest value [`MailboxListSnapshot`] skips.
// By reference because `skip_serializing_if` hands its predicate one; a
// by-value signature, which clippy suggests for a copy this small, no longer
// fits the call serde generates.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// Whether the view mode is the default flat one, which
/// [`MailboxListSnapshot`] skips.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_flat(mode: &ViewMode) -> bool {
    *mode == ViewMode::Flat
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
