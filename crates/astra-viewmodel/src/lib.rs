// SPDX-License-Identifier: GPL-3.0-only
//! Pure view-models shared by every Astra Mail surface.
//!
//! Immutable snapshots a host renders, projected from the engine's domain types. They
//! carry no runtime, no engine handle, and no host dependency: state lives in the
//! engine, grouping and ordering live here, so every surface shares one definition of
//! what is on screen.

mod compose_request;
mod drafts;
mod folders;
mod reading;
mod rows;
mod sending;
mod sync_progress;
pub mod view;

pub use compose_request::{ComposeKind, ComposeRequestSnapshot};
pub use drafts::{DraftSaveState, DraftStatusSnapshot};
pub use folders::{folder_role, sorted_folder_rows, AccountFolderRow, FolderRole, FolderRow};
pub use reading::{AttachmentRow, MessageKey, NotAMessageRow, ReadingSnapshot};
pub use rows::{MessageRowView, RowKey, ViewMode};
pub use sending::{queued_row, QueuedKey, QueuedRow, QueuedState, SendingSnapshot};
pub use sync_progress::{AccountSyncProgress, AccountSyncState, SyncProgressSnapshot};
pub use view::MailboxListSnapshot;
