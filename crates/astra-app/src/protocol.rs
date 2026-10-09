// SPDX-License-Identifier: GPL-3.0-only
//! The host-facing protocol of the unidirectional loop: the [`Surface`]s a host observes,
//! the [`AppObserver`] it implements to learn one changed, and the [`Intent`] it
//! dispatches. These are the pure types the runtime drives; each crosses the IPC as JSON.

mod intent;
mod intent_drafts;
mod status;

pub use intent::{FolderIntent, Intent, ViewMode};
pub use intent_drafts::{CompositionId, DraftsIntent};

use serde::{Deserialize, Serialize};

/// A surface a host observes and pulls an immutable snapshot for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The mailbox list: the folder pane and the mail rows.
    MailboxList,
    /// Background mail-download progress; drives a "downloading Y of X" bar while a sync
    /// is running.
    SyncProgress,
    /// The reading pane: the one open message's sanitised body, or nothing when no
    /// message is open. Every publish is one message's body, so the host repaints the
    /// pane on every signal rather than debouncing them.
    Reading,
    /// The draft-save hint of the composition that saved: whether its words are on the
    /// server or still on their way. Every publish is one transition of one
    /// composition's save, so the host repaints the composer's hint on every signal
    /// rather than debouncing them; `None` from the pull means no composition has a
    /// save to report.
    DraftStatus,
}

/// A host implements this to learn a [`Surface`] changed, then pulls its snapshot.
///
/// Implementations must be cheap and non-blocking (e.g. hop to the UI thread and
/// re-render); the app awaits nothing on them.
pub trait AppObserver: Send + Sync {
    /// Signals that `surface`'s snapshot changed.
    fn surface_changed(&self, surface: Surface);
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
