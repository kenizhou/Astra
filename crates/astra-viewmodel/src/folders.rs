// SPDX-License-Identifier: GPL-3.0-only
//! Sidebar folder ordering and folder-row types for the mailbox-list view-model.
//!
//! Orders an account's mailboxes so the role-bearing special folders lead in a fixed
//! canonical order ahead of every custom folder, so the folder tree reads identically on
//! every surface regardless of the (arbitrary) order the provider lists folders in;
//! grouping and ordering live here.
//!
//! [`FolderRole`] and [`FolderRow`] live here because they model folder identity and
//! metadata: the natural home alongside the ordering logic, and are re-exported from the
//! crate root.

use std::collections::{BTreeMap, BTreeSet};

use engine_api::{AccountId, Mailbox, MailboxId, MailboxRole};
use serde::{Deserialize, Serialize};

/// The special role a folder plays, mirroring RFC 6154 SPECIAL-USE and the JMAP
/// equivalents.
///
/// Exposed on [`FolderRow`] so surfaces can badge or group well-known folders without
/// hard-coding name heuristics. Crosses the IPC as a snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderRole {
    /// The primary inbox.
    Inbox,
    /// Drafts: messages in progress.
    Drafts,
    /// Sent: a copy of every sent message.
    Sent,
    /// Archive: long-term storage.
    Archive,
    /// Junk or spam: the server-side spam filter destination.
    Junk,
    /// Trash: recoverable deleted messages.
    Trash,
    /// Another role-bearing special folder (flagged, all, important, and the rest).
    Other,
}

/// One sidebar folder: its key, display name, optional special role, unread count, and
/// where it sits in the account's folder tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderRow {
    /// The mailbox's provider key (stable identity, used to select it).
    pub key: MailboxId,
    /// The folder's display name: the folder's own name, never the path to it. Every
    /// adapter gives the segment alone, so the row says `2024` and the indent says which
    /// Archive it is in.
    pub name: String,
    /// The folder's special role, or `None` for an ordinary custom folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<FolderRole>,
    /// How many messages in the folder are unread, as the **server** counts them; so it
    /// covers mail older than the synced window, which is what makes it the same number
    /// the user's other mail client shows.
    ///
    /// `0` means show no badge, and folds together the two cases that render
    /// identically: nothing is unread, and the provider reported no count at all. A
    /// surface hides the badge at zero rather than drawing one, so distinguishing them
    /// would change no pixel.
    pub unread: u32,
    /// The key of the row this one is drawn underneath, or `None` at the top of the
    /// account's tree.
    ///
    /// The row's drawn parent, which is not always the server's: a role-bearing folder
    /// is drawn at the top however the provider files it, and a folder whose parent this
    /// account does not list has nowhere else to go ([`sorted_folder_rows`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<MailboxId>,
    /// How many rows up the chain of parents reaches the top: `0` for a top-level
    /// folder, `1` for one filed inside it, and so on. What a surface multiplies by one
    /// indent step.
    pub depth: u32,
    /// Whether any other row in this account names this one as its
    /// [`parent`](FolderRow::parent).
    ///
    /// What decides whether the row draws a disclosure control at all: a folder that
    /// holds only mail has nothing to open, and a chevron beside it would promise a
    /// tree that is not there.
    pub has_children: bool,
    /// Whether this folder's own children are showing. Meaningless, and always `false`,
    /// where [`has_children`](FolderRow::has_children) is `false`.
    ///
    /// The core's answer, persisted per account and folder; a surface renders this and
    /// asks the core to change it rather than keeping a copy that would disagree with
    /// the other surfaces and reset on every restart.
    pub expanded: bool,
}

/// One account's sorted folder list; used by the folder pane to show every account's
/// folders at once, with each account as an expandable group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountFolderRow {
    /// The account's stable id.
    pub account_id: AccountId,
    /// The label the pane shows on this account's group header: the account's email
    /// address, or the name its owner sends mail under when one is set.
    ///
    /// A label, never an identity: `account_id` is what selects the account, and this is
    /// what a person recognises it by.
    pub display_name: String,
    /// The account's sending address: what a composition's From picker shows
    /// beside the label, the address mail leaves from. Absent from a snapshot
    /// an older emitter built, which reads as empty.
    #[serde(default)]
    pub address: String,
    /// Whether this account's folder tree is open in the sidebar.
    ///
    /// Independent of selection, and persisted across launches. A surface renders the
    /// chevron from this and asks the core to change it; it keeps no expansion state of
    /// its own, or the tree would disagree with itself between surfaces and across a
    /// restart.
    pub expanded: bool,
    /// Whether the account's provider can change its folder tree: what decides whether
    /// the pane offers the folder edit verbs at all. Absent reads as `false`, so a
    /// provider that cannot have its tree edited never shows the verbs, and a snapshot
    /// from before the field carries nothing rather than a lying `true`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub can_edit_folders: bool,
    /// Whether the account's provider can report its mail (the junk and
    /// not-junk verdicts this slice surfaces): what decides whether the row
    /// menu offers Mark as spam and the verb row offers Not spam at all.
    /// Absent reads as `false`, the same honesty rule `can_edit_folders`
    /// holds, so a snapshot from before the field carries nothing rather
    /// than a lying `true`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub can_report_mail: bool,
    /// The account's sorted folder rows, ready for the sidebar.
    pub folders: Vec<FolderRow>,
}

/// Whether the capability flag is off, the only shape the wire omits.
// By reference because `skip_serializing_if` hands its predicate one; a
// by-value signature, which clippy suggests for a copy this small, no longer
// fits the call serde generates.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// One mailbox's client-visible role, or `None` for an ordinary folder.
#[must_use]
pub fn folder_role(mailbox: &Mailbox) -> Option<FolderRole> {
    mailbox.role.as_ref().map(mailbox_role_to_folder_role)
}

fn folder_row(
    mailbox: &Mailbox,
    parent: Option<&MailboxId>,
    depth: u32,
    has_children: bool,
) -> FolderRow {
    FolderRow {
        key: mailbox.id.clone(),
        name: mailbox.name.clone(),
        role: folder_role(mailbox),
        unread: mailbox.unread_count.unwrap_or(0),
        parent: parent.cloned(),
        depth,
        has_children,
        // Open until the publisher stamps the persisted state over it; see
        // `FolderRow::expanded`.
        expanded: has_children,
    }
}

/// Maps an engine [`MailboxRole`] to the client-visible [`FolderRole`].
fn mailbox_role_to_folder_role(role: &MailboxRole) -> FolderRole {
    match role {
        MailboxRole::Inbox => FolderRole::Inbox,
        MailboxRole::Drafts => FolderRole::Drafts,
        MailboxRole::Sent => FolderRole::Sent,
        MailboxRole::Archive => FolderRole::Archive,
        MailboxRole::Junk => FolderRole::Junk,
        MailboxRole::Trash => FolderRole::Trash,
        MailboxRole::Flagged
        | MailboxRole::Important
        | MailboxRole::All
        | MailboxRole::Other(_) => FolderRole::Other,
    }
}

/// The account's folders as a **tree**, flattened depth-first into the order a pane
/// draws them: each folder immediately followed by the folders filed inside it.
///
/// Siblings are ordered the way the whole list always is: the role-bearing special
/// mailboxes lead, in a fixed canonical order (Inbox, Drafts, Sent, and so on to
/// Trash), ahead of every other folder, which then follow by the provider's sort hint
/// and then case-insensitive name. Ordering lives here, in the shared view-model, so the
/// folder tree reads identically on every surface regardless of the (arbitrary) order
/// the provider lists folders in.
///
/// Flat rather than nested because a `Vec<FolderRow>` crosses the IPC and a flat list is
/// what a pane draws anyway; [`depth`](FolderRow::depth) is what a surface indents by,
/// and [`parent`](FolderRow::parent) is what a tree-shaped widget re-nests from.
#[must_use]
pub fn sorted_folder_rows(folders: &[Mailbox]) -> Vec<FolderRow> {
    let known: BTreeSet<&MailboxId> = folders.iter().map(|mailbox| &mailbox.id).collect();
    let mut children: BTreeMap<&MailboxId, Vec<&Mailbox>> = BTreeMap::new();
    let mut top: Vec<&Mailbox> = Vec::new();
    for mailbox in folders {
        match drawn_under(mailbox, &known) {
            Some(parent) => children.entry(parent).or_default().push(mailbox),
            None => top.push(mailbox),
        }
    }
    top.sort_by_key(|mailbox| folder_sort_key(mailbox));
    for group in children.values_mut() {
        group.sort_by_key(|mailbox| folder_sort_key(mailbox));
    }

    let mut rows = Vec::with_capacity(folders.len());
    let mut seen: BTreeSet<&MailboxId> = BTreeSet::new();
    let mut stack: Vec<(&Mailbox, Option<&MailboxId>, u32)> =
        top.into_iter().rev().map(|m| (m, None, 0)).collect();
    while let Some((mailbox, parent, depth)) = stack.pop() {
        let key = &mailbox.id;
        if !seen.insert(key) {
            continue;
        }
        let nested: &[&Mailbox] = children.get(key).map_or(&[], Vec::as_slice);
        rows.push(folder_row(mailbox, parent, depth, !nested.is_empty()));
        // Reversed, because the stack pops last-in first: this is what puts a folder's
        // own children directly beneath it, in their sorted order.
        stack.extend(
            nested
                .iter()
                .rev()
                .map(|child| (*child, Some(key), depth + 1)),
        );
    }

    // A folder the walk never reached is one in a cycle, which no server should be able
    // to report. It still gets a row, at the top: a folder missing from the pane is a
    // folder whose mail the user has no way to open.
    for mailbox in folders {
        if seen.insert(&mailbox.id) {
            rows.push(folder_row(mailbox, None, 0, false));
        }
    }
    rows
}

/// The key of the row this folder is **drawn** underneath, which is not always the one
/// the server files it under. `None` puts it at the top of the account's tree.
///
/// Two folders go to the top although their provider nests them:
///
/// - A role-bearing folder. Gmail files Sent, Drafts, Trash and All Mail inside a
///   `[Gmail]` container over IMAP, and Exchange has its own arrangements; burying the
///   folder a person reaches for most inside something their provider invented is what
///   no mail client does. Drawing these where the role says rather than where the path
///   says is the same decision as giving them the role's name.
/// - An orphan, whose parent this account does not list: an unsubscribed or hidden
///   intermediate. It has to go somewhere, and it cannot go under a row that is not
///   there.
fn drawn_under<'a>(mailbox: &'a Mailbox, known: &BTreeSet<&'a MailboxId>) -> Option<&'a MailboxId> {
    if mailbox.role.as_ref().and_then(role_rank).is_some() {
        return None;
    }
    let parent = mailbox.parent.as_ref()?;
    known.contains(parent).then_some(parent)
}

/// The sort key for one folder: role-bearing folders form the first group (`0`) ordered
/// by their [`role_rank`]; all others form the second group (`1`). Within each group the
/// provider's `sort_order` then the case-insensitive name break ties: so two custom
/// folders sort by name, and a provider that supplies sort hints (JMAP) is honoured.
fn folder_sort_key(mailbox: &Mailbox) -> (u8, u8, u32, String) {
    let (group, rank) = match mailbox.role.as_ref().and_then(role_rank) {
        Some(rank) => (0, rank),
        None => (1, 0),
    };
    (group, rank, mailbox.sort_order, mailbox.name.to_lowercase())
}

/// The canonical sidebar position of a recognised special-use role, or `None` for an
/// unrecognised ([`MailboxRole::Other`]) role, which is treated as an ordinary folder
/// rather than a special one. The match is exhaustive on purpose: a new engine role must
/// be given an explicit position here (the build breaks until it is) rather than
/// silently falling among the custom folders.
fn role_rank(role: &MailboxRole) -> Option<u8> {
    Some(match role {
        MailboxRole::Inbox => 0,
        MailboxRole::Drafts => 1,
        MailboxRole::Sent => 2,
        MailboxRole::Archive => 3,
        MailboxRole::Junk => 4,
        MailboxRole::Trash => 5,
        MailboxRole::Flagged => 6,
        MailboxRole::Important => 7,
        MailboxRole::All => 8,
        MailboxRole::Other(_) => return None,
    })
}

#[cfg(test)]
#[path = "folders_tests.rs"]
mod tests;
