// SPDX-License-Identifier: GPL-3.0-only
//! The drafts half of the inbound protocol ([`DraftsIntent`]), which [`Intent::Drafts`]
//! carries.
//!
//! Split from [`super::intent`] to keep each file under the 500-line limit, following
//! [`FolderIntent`](super::FolderIntent). Drafts read as a unit for the reason that one
//! does: one composer, one stored copy on the server, and three verbs that mean nothing
//! anywhere else in the app.

use astra_composer::ComposerDocument;
use engine_api::AccountId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One open composition, named by the host.
///
/// Minted by the client when it opens a composer and kept for as long as that composer
/// lives. A desktop can have several composers open at once, each saving a different
/// draft, so the id is what keeps their stored copies apart.
///
/// Opaque to the core, which uses it only to look up what it already knows about the
/// composition. It is never sent anywhere: the draft on the server is named by the
/// provider's own key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompositionId(String);

impl CompositionId {
    /// Names a composition, rejecting a blank id. The value crosses the IPC from a host,
    /// so it is checked rather than trusted; a blank one would merge every composer's
    /// draft into one entry and each save would supersede a different composer's copy.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty()).then_some(Self(value))
    }

    /// The id as the host gave it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CompositionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CompositionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Accepts any string the constructor accepts, and nothing else.
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = CompositionId;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a non-blank composition id")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                CompositionId::new(value)
                    .ok_or_else(|| E::custom("a composition id cannot be blank"))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// What the composer asks the runtime to do with the draft it is holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftsIntent {
    /// Store the composer's current content in the account's Drafts folder, replacing
    /// what the previous save of this composition left there.
    ///
    /// Dispatched both by the composer's explicit "Save as draft" and by its idle timer;
    /// the two are the same operation, and the core cannot tell them apart. A save whose
    /// content is unchanged since the last one reaches no server, and is still honestly
    /// saved.
    Save {
        /// Which composer this is, so the save supersedes its own previous copy and no
        /// other's.
        composition: CompositionId,
        /// The account to store it under: the composer's From dropdown, one of the
        /// configured accounts.
        ///
        /// Fixed by the first save of a composition: a draft already on the server is
        /// stored in one account's folder, and honouring a later change here would leave
        /// the first copy behind in the account it was written to.
        from: AccountId,
        /// The `To` recipients, comma-separated. May be empty: a draft is unfinished by
        /// definition, and refusing to keep one that has no recipient yet is refusing to
        /// keep the ones most worth keeping.
        to: String,
        /// The `Cc` recipients, comma-separated; may be empty.
        cc: String,
        /// The `Bcc` recipients, comma-separated; may be empty.
        bcc: String,
        /// The subject line; may be empty.
        subject: String,
        /// The shared composer document.
        ///
        /// Rendered by the composer crate's deterministic renderer on the way out, so a
        /// stored draft is byte-identical to a sent one built from the same document.
        /// Attachment blobs are deferred to the attachments slice: a document this slice
        /// carries may name handles, and their bytes join the stored copy when that slice
        /// lands.
        document: ComposerDocument,
        /// Forget the composition once the save has settled: the composer is being left,
        /// by a click elsewhere or a closed window, and the draft stays in Drafts.
        ///
        /// Part of the save rather than a [`DraftsIntent::Close`] after it, because each
        /// intent is its own task. A close that landed first would leave the save with no
        /// stored key to supersede, and the server would keep the old copy beside the new
        /// one.
        then_close: bool,
    },
    /// Remove this composition's stored draft from the server and forget the composition.
    ///
    /// A composition that was never saved reaches no server: there is nothing there to
    /// remove.
    Discard {
        /// The composer whose stored draft to remove.
        composition: CompositionId,
    },
    /// Save what the composition last handed over, then forget it, leaving what is on
    /// the server alone for the user to find in Drafts.
    ///
    /// Not optional bookkeeping. Without it the core holds a record per composer for the
    /// life of the process, and the next composer the host opens under a **reused** id
    /// would supersede a draft the user had finished with.
    Close {
        /// The composer that closed.
        composition: CompositionId,
    },
}
