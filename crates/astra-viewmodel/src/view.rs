// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox-list view-model: an immutable snapshot a surface renders.
//!
//! Pure projection over the engine's account and mailbox state; state lives in
//! the engine, the surface renders the snapshot. Grouping and ordering happen
//! here so every surface shares one definition of what is on screen. This file
//! holds the snapshot type; the folder-row projection itself lives in
//! `folders`, the message-row types in `rows`.

use serde::{Deserialize, Serialize};

use engine_api::AccountId;

use crate::folders::AccountFolderRow;
use crate::rows::{MessageRowView, RowKey, ViewMode};
use crate::search::SearchState;

/// Whose mail the list is standing in, as the snapshot's header carries it:
/// every account's inboxes merged, one account's whole mailbox, or one
/// folder of one account. The account switcher's own wire: a surface that
/// renders the list names whose mail stands from this, never from a
/// mirror it keeps alone, so a relaunch and a second surface read the
/// same label the core holds.
///
/// The folder half is a mirror of the app's folder reference (the account
/// and the key together), stated here in the view-model's own words
/// because the app's reference type cannot cross the dependency; the
/// account and key fields are pinned equal to it in the tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListScope {
    /// The unified view: every account's inbox rows merged, newest first.
    AllInboxes,
    /// One account's whole mailbox, the switcher's selection.
    Account {
        /// The account whose mail the list stands in.
        account: AccountId,
    },
    /// One folder of one account, the tree's own selection.
    Folder {
        /// The account that owns the folder.
        account: AccountId,
        /// The folder's key, unique within `account`.
        key: String,
    },
}

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
    /// The mailbox list's search state, present exactly while a search is
    /// active.
    ///
    /// Absent on the wire when no search is running, the at-rest value, so a
    /// snapshot nobody searched with crosses byte-identically to the shape it
    /// carried before search existed, and an old emitter's JSON parses to
    /// `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchState>,
    /// Whose mail the list is standing in: the unified view, one account,
    /// or one folder. Absent on a wire from a build before the field,
    /// which reads back as no scope named and a surface falls back to its
    /// own state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_scope: Option<ListScope>,
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
