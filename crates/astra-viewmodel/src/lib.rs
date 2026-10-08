// SPDX-License-Identifier: GPL-3.0-only
//! Pure view-models shared by every Astra Mail surface.
//!
//! Immutable snapshots a host renders, projected from the engine's domain types. They
//! carry no runtime, no engine handle, and no host dependency: state lives in the
//! engine, grouping and ordering live here, so every surface shares one definition of
//! what is on screen.

mod folders;
mod sync_progress;
pub mod view;

pub use folders::{folder_role, sorted_folder_rows, AccountFolderRow, FolderRole, FolderRow};
pub use sync_progress::{AccountSyncProgress, AccountSyncState, SyncProgressSnapshot};
pub use view::MailboxListSnapshot;
