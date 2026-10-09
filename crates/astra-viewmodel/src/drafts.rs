// SPDX-License-Identifier: GPL-3.0-only
//! The draft-status view-model: the immutable snapshot of one composition's
//! most recent save that a surface renders as the composer's quiet hint.
//!
//! A hint and never a gate: no state here stops a composer being closed, and
//! none of it is worth a dialog. The snapshot names the composition whose save
//! moved, so a surface matching it against its own composer discards a stale
//! one, the same rule the reading pane follows for a stale body.

use serde::{Deserialize, Serialize};

/// The state of a composition's most recent draft save.
///
/// Two states this slice: a save in flight, and one that landed. A save the
/// outbox is holding for a lost network reads as [`Self::Saving`], because the
/// words are on their way and the composer should say so; the queued and failed
/// split arrives with the sending slice's richer status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftSaveState {
    /// A save is in flight, including one parked in the outbox waiting for a
    /// network to carry it.
    Saving,
    /// The draft is on the server. Also the answer to a save whose content was
    /// unchanged, which reaches no server and is still honestly saved.
    Saved,
}

/// An immutable snapshot of one composition's most recent draft save, for a
/// surface to render as the composer's quiet hint.
///
/// `None` from the surface's pull means no composition has a save to report:
/// nothing has been saved this session, or the composition closed and its hint
/// went with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftStatusSnapshot {
    /// The composition whose save moved, exactly as the host minted its id, so
    /// a surface matches the snapshot against its own composer and ignores a
    /// stale one.
    pub composition: String,
    /// How the most recent save of that composition ended up.
    pub state: DraftSaveState,
    /// When the save landed, as a whole-second UTC `…Z` instant
    /// (`2026-10-09T08:30:00Z`); empty while the save is still in flight. A
    /// surface localises it for display.
    pub saved_at: String,
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod tests;
