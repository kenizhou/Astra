// SPDX-License-Identifier: GPL-3.0-only
//! The attachment byte read: the one downloadable part's decoded bytes and
//! suggested file name, read through the engine's cache-first fetch. The
//! reading snapshot lists the metadata rows; this is the read a host's
//! Save As drives once the reader picks one row.

use engine_api::{AccountId, AttachmentPartId, Provider, ProviderKey};

use crate::{reading::resolve, reference::MessageRef, App};

/// One downloadable attachment as a host's save needs it: the row's
/// suggested file name beside the decoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentContent {
    /// The suggested file name, from the part's headers: the name a save
    /// dialog opens on, the same name the reading snapshot's row carries.
    pub file_name: String,
    /// The content-transfer-decoded bytes, ready to write.
    pub bytes: Vec<u8>,
}

/// Why an attachment read cannot answer bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentReadError {
    /// The message is not in the account's synced set, or the account or
    /// its provider is gone: there is nothing to resolve the part id
    /// against.
    Unresolved,
    /// The engine's fetch refused; the sentence is the engine's own.
    Fetch(String),
}

impl core::fmt::Display for AttachmentReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unresolved => f.write_str("the message could not be resolved"),
            Self::Fetch(sentence) => write!(f, "the attachment could not be fetched: {sentence}"),
        }
    }
}

impl<P: Provider> App<P> {
    /// Reads one attachment's decoded bytes: the part `id` names (the id the
    /// reading snapshot's attachment row carries) of the message `key`
    /// names in `account`. `Ok(None)` when the message resolves and its
    /// source reads but no downloadable part matches the id.
    ///
    /// # Errors
    ///
    /// Returns [`AttachmentReadError::Unresolved`] when the account, its
    /// provider, or the message cannot be resolved, and
    /// [`AttachmentReadError::Fetch`] when the engine's fetch refuses.
    pub async fn attachment_content(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        id: u32,
    ) -> Result<Option<AttachmentContent>, AttachmentReadError> {
        let handle = self
            .account_handle(account)
            .await
            .ok_or(AttachmentReadError::Unresolved)?;
        let provider = handle
            .providers
            .first()
            .ok_or(AttachmentReadError::Unresolved)?;
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        let original = resolve(&self.engine, &message)
            .await
            .ok_or(AttachmentReadError::Unresolved)?;
        self.engine
            .message_attachment(provider, account, &original, AttachmentPartId::new(id))
            .await
            .map_err(|err| AttachmentReadError::Fetch(err.to_string()))
            .map(|content| {
                content.map(|content| AttachmentContent {
                    file_name: content.attachment().file_name().to_owned(),
                    bytes: content.into_bytes(),
                })
            })
    }
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
