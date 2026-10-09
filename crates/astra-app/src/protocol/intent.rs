// SPDX-License-Identifier: GPL-3.0-only
//! The [`Intent`] enum: the single inbound channel of the unidirectional loop, with the
//! enums its variants carry ([`ViewMode`], [`FolderIntent`], and
//! [`DraftsIntent`](super::DraftsIntent)) beside it or one module away.

use astra_composer::ComposerDocument;
use engine_api::AccountId;
use serde::{Deserialize, Serialize};

use crate::reference::{FolderRef, MessageRef, RowRef};

use super::{ComposerBlob, DraftsIntent, OutboxIntent};

/// How the mailbox list is grouped.
///
/// Defined in the view-model, beside the rows it groups, so the intent that
/// changes the mode and the snapshot that carries it name one type; re-exported
/// here to keep the protocol the one import a host needs.
pub use astra_viewmodel::ViewMode;

/// What the folder pane asks the runtime to do to an account's folders.
///
/// Each change goes through the engine's outbox, so one made offline is drawn in the pane
/// at once, marked pending, and reaches the server when the device is back online.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FolderIntent {
    /// Make a folder called `name` inside `parent`, or at the top of the account's tree.
    Create {
        /// The account to make it in.
        account: AccountId,
        /// The folder to make it inside, a key of `account`'s, or `None` for the top level.
        #[serde(skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// The new folder's own name.
        name: String,
    },
}

/// A host intent: the single inbound channel of the unidirectional loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    /// Sync the account's mail (every connected folder) and refresh the mailbox-list
    /// snapshot.
    RefreshMail,
    /// Show one account's folders (by account id), or the unified "all inboxes" view
    /// (`None`). Resets the selected folder.
    SelectAccount(Option<AccountId>),
    /// Open or shut one account's folder tree in the sidebar, and remember it across
    /// launches.
    ///
    /// Deliberately **not** navigation: it changes neither the selected account nor the
    /// selected folder, so any number of trees can be open at once. A client renders the
    /// chevron from the account's own row and never keeps its own copy, which would
    /// disagree with the core and reset on every restart.
    SetAccountExpanded {
        /// The account whose tree to open or shut.
        account: AccountId,
        /// Whether the tree is open.
        expanded: bool,
    },
    /// Open or shut the folders filed inside one folder, and remember it across launches.
    /// An account's tree and a folder's are the same thing one level down, so this follows
    /// [`Intent::SetAccountExpanded`]'s rules: not navigation, independent of what is
    /// selected, and the core's to remember. A [`FolderRef`] for [`Intent::SelectFolder`]'s
    /// reason.
    SetFolderExpanded {
        /// The folder whose own sub-folders to show or hide, bound to its owning account.
        folder: FolderRef,
        /// Whether the folders inside it are showing.
        expanded: bool,
    },
    /// Show one folder's mail. A [`FolderRef`], never a bare key: a key is unique only
    /// within its account, and there is no folder-only form. An account's own all-mail
    /// view is [`Intent::SelectAccount`].
    SelectFolder {
        /// The folder to show, bound to its owning account.
        folder: FolderRef,
    },
    /// Switch the mailbox list between a flat and a threaded view.
    SetViewMode(ViewMode),
    /// Grow the visible mailbox-list window by one page: the host dispatches this as it
    /// scrolls toward the end of the list, so only the rows in view are built and crossed
    /// the IPC. Any navigation that changes the list (select account or folder, switch
    /// view mode) resets the window to the first page.
    ShowMore,
    /// Mark one row read (`read`) or unread: a message in the flat list, or a whole
    /// conversation in the threaded list (every message on the thread). The write goes
    /// through the engine's outbox, so the flipped row state shows at once and reaches
    /// the server when the device can.
    MarkRead {
        /// The row to mark, bound to its owning account.
        row: RowRef,
        /// Whether the row's mail counts as read.
        read: bool,
    },
    /// Flag one row (`flagged`) or unflag it: a message in the flat list, or a whole
    /// conversation in the threaded list. As [`Intent::MarkRead`], an outbox write whose
    /// outcome the republished snapshot carries.
    SetFlagged {
        /// The row to flag, bound to its owning account.
        row: RowRef,
        /// Whether the row's mail carries the flag.
        flagged: bool,
    },
    /// Read one message in the reading pane: fetch its body, sanitise the HTML,
    /// resolve the inline images, and publish the reading snapshot. Opening also
    /// marks the message read through the same write path [`Intent::MarkRead`]
    /// drives, unless it already is. A [`MessageRef`], never a bare key: the
    /// body read routes through the message's owning account.
    OpenMessage {
        /// The message to read, bound to its owning account.
        message: MessageRef,
    },
    /// Shut the reading pane: the open message's body is forgotten and the surface
    /// goes empty. Not the same as navigating the list, which leaves the pane as it
    /// was.
    CloseReading,
    /// A change to an account's folder tree ([`FolderIntent`]).
    Folders(FolderIntent),
    /// Send the composer's rich document through the durable outbox: the words
    /// are queued before any server is asked, so a send the app does not
    /// survive is neither lost nor delivered twice. The outcome reaches the
    /// user through the [`Sending`](super::Surface::Sending) surface, never a
    /// return value.
    SubmitRichMail {
        /// The account to send from: the composer's From dropdown, one of the
        /// configured accounts.
        from: AccountId,
        /// The `To` recipients, comma-separated.
        to: String,
        /// The `Cc` recipients, comma-separated; may be empty.
        cc: String,
        /// The `Bcc` recipients, comma-separated; may be empty.
        bcc: String,
        /// The subject line; may be empty.
        subject: String,
        /// The shared composer document, rendered by the composer crate's
        /// deterministic renderer on the way out, so a sent message is
        /// byte-identical to a draft stored from the same document.
        document: ComposerDocument,
        /// Host-resolved bytes for every attachment handle the document
        /// references; empty while the editor produces no attachments. A
        /// handle with no bytes fails the send rather than sending a message
        /// silently missing its files.
        blobs: Vec<ComposerBlob>,
    },
    /// What the outbox panel asks the runtime to do with a queued send
    /// ([`OutboxIntent`]): show the queue, or act on one send in it.
    Outbox(OutboxIntent),
    /// What the composer asks the runtime to do with the draft it is holding
    /// ([`DraftsIntent`]): save it, discard it, or save and close.
    Drafts(DraftsIntent),
}
