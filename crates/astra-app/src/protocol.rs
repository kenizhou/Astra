// SPDX-License-Identifier: GPL-3.0-only
//! The host-facing protocol of the unidirectional loop: the [`Surface`]s a host observes,
//! the [`AppObserver`] it implements to learn one changed, and the [`Intent`] it
//! dispatches. These are the pure types the runtime drives; each crosses the IPC as JSON.

mod intent;
mod intent_compose;
mod intent_drafts;
mod intent_outbox;
mod status;

pub use intent::{FolderIntent, Intent, SearchScope, ViewMode};
pub use intent_compose::ComposeIntent;
pub use intent_drafts::{CompositionId, DraftsIntent};
pub use intent_outbox::OutboxIntent;

use astra_composer::DraftBlobHandle;
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
    /// The sends that have not gone out: every account's queued sends in one list.
    /// Every publish is one transition of the queue (a send joined it, left it, or
    /// changed state), so the host repaints the panel on every signal rather
    /// than debouncing them.
    Sending,
    /// The composition a reply or forward asked the host to open in its
    /// composer: the whole prefill (recipients, subject, threading header, the
    /// quoted original) as one thing. Every publish is one request replacing
    /// whatever it held, so the host repaints on every signal rather than
    /// debouncing them. `None` from the pull means no request is pending.
    ComposeRequest,
}

/// Host-resolved bytes for one composer blob handle.
///
/// A document's attachments name their bytes by an opaque handle, because the
/// bytes themselves are too large to cross the IPC inside the document; the
/// host resolves each handle and hands the bytes back beside the document at
/// submit, so the message that goes out carries its files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerBlob {
    /// The opaque blob handle the document's attachment named.
    pub handle: DraftBlobHandle,
    /// The attachment bytes for that handle.
    pub bytes: Vec<u8>,
}

impl ComposerBlob {
    /// Builds a resolved composer blob.
    #[must_use]
    pub fn new(handle: DraftBlobHandle, bytes: Vec<u8>) -> Self {
        Self { handle, bytes }
    }
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

// The submit family's threaded arms, split from the tests above so that file
// stays under the 500-line cap.
#[cfg(test)]
#[path = "protocol_submit_tests.rs"]
mod submit_tests;

// The search family's intents, split for the same reason.
#[cfg(test)]
#[path = "protocol_search_tests.rs"]
mod search_tests;
