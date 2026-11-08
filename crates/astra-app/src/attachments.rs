// SPDX-License-Identifier: GPL-3.0-only
//! The reading pane's two Save As reads: the one downloadable part's
//! decoded bytes, and the whole message's `.eml` source. Both answer a
//! suggested file name beside bytes that are ready to write, and both read
//! through the engine's cache-first fetch. The `.eml` leg is the engine's
//! own source read, the bytes the sender's server delivered, never a
//! re-serialization of the normalized projection; the snapshot lists the
//! metadata rows, and these are the reads a host's Save As drives once the
//! reader picks one.

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

/// One message's `.eml` export as a host's save needs it: the file name the
/// save dialog opens on, derived from the subject, beside the raw source
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmlContent {
    /// The suggested file name: the sanitised subject with the `.eml`
    /// suffix, `message.eml` when the subject sanitises to nothing.
    pub file_name: String,
    /// The raw RFC 5322 source exactly as delivered, ready to write.
    pub bytes: Vec<u8>,
}

/// Why an `.eml` read cannot answer bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmlReadError {
    /// The message is not in the account's synced set, or the account or
    /// its provider is gone: there is nothing to read the source of.
    Unresolved,
    /// The engine's source fetch refused; the sentence is the engine's own.
    Fetch(String),
}

impl core::fmt::Display for EmlReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unresolved => f.write_str("the message could not be resolved"),
            Self::Fetch(sentence) => {
                write!(f, "the message source could not be fetched: {sentence}")
            }
        }
    }
}

/// The stem cap for a suggested `.eml` name, in characters: far under
/// every filesystem's name limit for ascii, and a stem of wider
/// characters that overruns a byte-counted limit answers the typed
/// failure rather than a corrupt write.
const EML_STEM_CAP: usize = 80;

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

    /// Reads one message's `.eml` export: the raw RFC 5322 source the
    /// sender's server delivered, exactly as the engine holds it (never a
    /// re-serialization, which would change the wording of headers, the
    /// order of parts and the transfer encodings), beside the file name the
    /// save dialog opens on, derived from the message's subject. The
    /// message `key` names in `account`; the reading snapshot's reference
    /// carries both.
    ///
    /// # Errors
    ///
    /// Returns [`EmlReadError::Unresolved`] when the account, its provider,
    /// or the message cannot be resolved, and [`EmlReadError::Fetch`] when
    /// the engine's source fetch refuses.
    pub async fn eml_content(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<EmlContent, EmlReadError> {
        let handle = self
            .account_handle(account)
            .await
            .ok_or(EmlReadError::Unresolved)?;
        let provider = handle.providers.first().ok_or(EmlReadError::Unresolved)?;
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        let original = resolve(&self.engine, &message)
            .await
            .ok_or(EmlReadError::Unresolved)?;
        let source = self
            .engine
            .message_source(provider, account, &original)
            .await
            .map_err(|err| EmlReadError::Fetch(err.to_string()))?;
        Ok(EmlContent {
            file_name: suggested_eml_name(original.envelope.subject.as_deref().unwrap_or("")),
            bytes: source.into_bytes(),
        })
    }
}

/// The file name a Save as .eml suggests for `subject`: every character a
/// file name cannot carry (path separators, the characters Windows forbids,
/// control characters) folds to a space, whitespace runs collapse to one,
/// the stem sheds trailing dots and spaces and clamps at [`EML_STEM_CAP`]
/// characters, and the `.eml` suffix lands. A subject that sanitises to
/// nothing, no subject at all, or one whose stem is a Windows reserved
/// device name (`CON`, `NUL`, the COM and LPT ports, whose dotted forms the
/// write would refuse) suggests `message.eml`.
fn suggested_eml_name(subject: &str) -> String {
    let mut stem = String::new();
    let mut space = false;
    for ch in subject.chars() {
        let safe = ch.is_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '(' | ')' | ',' | '\'' | '+' | '&');
        if !safe || ch.is_whitespace() {
            space = true;
        } else {
            if space && !stem.is_empty() {
                stem.push(' ');
            }
            space = false;
            stem.push(ch);
        }
    }
    let stem: String = stem.chars().take(EML_STEM_CAP).collect();
    let stem = stem.trim_end_matches(['.', ' ']);
    if stem.is_empty() || is_windows_reserved(stem) {
        "message.eml".to_owned()
    } else {
        format!("{stem}.eml")
    }
}

/// Whether `stem` is one of the device names Windows refuses as a file
/// name whatever extension follows it, compared case-insensitively the
/// way the platform does.
fn is_windows_reserved(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
