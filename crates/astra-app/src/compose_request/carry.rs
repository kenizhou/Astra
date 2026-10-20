// SPDX-License-Identifier: GPL-3.0-only
//! The forward's carried attachments: the original's parts the Forward
//! intent named, pre-attached onto the published document as removable rows
//! whose bytes the submit re-reads off the original, so nothing crosses the
//! host boundary twice. A row carries the engine's part id behind a blob
//! handle the composer treats as opaque (`carry:<id>`); a row the composer
//! removed drops its handle from the document, and the part simply does not
//! ride the send.

use astra_composer::{
    AttachmentDisposition, AttachmentId, ComposerDocument, DraftAttachment, DraftBlobHandle,
};
use astra_viewmodel::ComposeRequestSnapshot;
use engine_api::{AccountId, Engine, Message, Provider};

/// The blob handle naming the original's part `id`.
pub(crate) fn handle_of(id: u32) -> String {
    format!("carry:{id}")
}

/// The carried ids whose handles still stand in the document: the rows the
/// composer left alone. A removed row dropped its handle, so its id drops
/// here too, before anything is read off the original.
pub(crate) fn standing(carry: &[u32], document: &ComposerDocument) -> Vec<u32> {
    carry
        .iter()
        .copied()
        .filter(|id| {
            document.attachments.iter().any(|row| {
                row.blob
                    .as_ref()
                    .is_some_and(|h| h.as_str() == handle_of(*id))
            })
        })
        .collect()
}

/// One carried part as the document's pre-attached row.
fn row(id: u32, file_name: &str, media_type: &str, size: u64) -> Option<DraftAttachment> {
    Some(DraftAttachment {
        id: AttachmentId::new(format!("carry-{id}"))?,
        blob: Some(DraftBlobHandle::new(handle_of(id))?),
        file_name: file_name.to_owned(),
        media_type: media_type.to_owned(),
        size: Some(size),
        disposition: AttachmentDisposition::Attachment,
        data_url: None,
    })
}

/// The carried parts of `original`, read through the engine as the rows the
/// forward's document opens holding. A carried id the original carries no
/// downloadable part for pre-attaches nothing: there is no row to show for
/// a part that is not there.
pub(crate) async fn carried_attachments<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    original: &Message,
    carry: &[u32],
) -> Vec<DraftAttachment> {
    if carry.is_empty() {
        return Vec::new();
    }
    let parts = engine
        .message_attachments(provider, account, original)
        .await
        .unwrap_or_default();
    carry
        .iter()
        .filter_map(|id| {
            parts
                .iter()
                .find(|part| part.id().as_u32() == *id)
                .and_then(|part| row(*id, part.file_name(), part.media_type(), part.size()))
        })
        .collect()
}

/// Appends the carried rows onto the published snapshot's document.
pub(crate) fn with_carried(
    mut snapshot: ComposeRequestSnapshot,
    carried: Vec<DraftAttachment>,
) -> ComposeRequestSnapshot {
    if carried.is_empty() {
        return snapshot;
    }
    snapshot.document.attachments.extend(carried);
    snapshot
}

#[cfg(test)]
#[path = "carry_tests.rs"]
mod tests;
