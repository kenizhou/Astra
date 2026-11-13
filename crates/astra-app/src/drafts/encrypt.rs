// SPDX-License-Identifier: GPL-3.0-only
//! The drafts-at-rest vault seam: the encrypted mirror a save writes before
//! the engine's own plaintext put, the delete a discard fires, and the read a
//! resume prefers. The engine persists its draft copies as plaintext and
//! cannot be changed, so the durable Astra-held copy is the vault-encrypted
//! mirror; resume reads it first and falls back to the engine body only for a
//! draft that was never mirrored. A mirror that exists but cannot be
//! decrypted fails closed (a banner, never the plaintext fallback).
//!
//! A child of [`super`], split out so the drafts and resume files stay under
//! the line cap. The store treats the payload as opaque bytes; this module is
//! where the engine's [`Draft`] meets JSON, both ways.
//!
//! **Privacy.** The draft is the user's mail. Nothing here logs its content;
//! the plaintext lives only in memory and inside the vault blob.

use astra_crypto_store::{
    delete_encrypted_draft, get_encrypted_draft, put_encrypted_draft, DraftMirrorError,
};
use astra_viewmodel::{MessageKey, ReadingSnapshot, SecurityBannerView, SecurityLevelView};
use engine_api::{AccountId, ApiError, Draft, EmailAddress, Message, Provider, SyncError};

use super::{CarriedPart, DraftResume};
use crate::{compose_request::carry, App};

/// Serialise the rendered `draft` and write the encrypted mirror under its
/// stable `Message-ID`. Runs BEFORE the engine's put: a vault error refuses
/// the save (fail closed, no plaintext persisted). An app with no crypto store
/// installed holds no mirror, so the save proceeds as a plain draft.
///
/// # Errors
/// Fails when the vault cannot wrap the draft or the mirror cannot be written.
pub(super) fn mirror_save<P: Provider>(
    app: &App<P>,
    account: &AccountId,
    draft: &Draft,
) -> Result<(), DraftMirrorError> {
    let Some(store) = app.crypto_store() else {
        return Ok(());
    };
    // The mirror's FK target: register the account idempotently, the way the
    // MIME wrapper does, so a plain account (no crypto wrapper installed) finds
    // the row standing on its first save. A store that cannot take it fails
    // closed.
    store
        .register_account(account.as_str(), draft.from.email.as_str())
        .map_err(DraftMirrorError::Store)?;
    let json = serde_json::to_vec(draft).map_err(|e| DraftMirrorError::Store(e.to_string()))?;
    put_encrypted_draft(&store, account.as_str(), draft.message_id.as_str(), &json)
}

/// Remove the mirror a discard takes with the stored copy. A missing row is
/// already gone; an app with no store holds no mirror. A delete error is logged
/// (counts and ids only) and never blocks the discard.
pub(super) fn mirror_delete<P: Provider>(app: &App<P>, account: &AccountId, draft_key: &str) {
    let Some(store) = app.crypto_store() else {
        return;
    };
    if let Err(err) = delete_encrypted_draft(&store, account.as_str(), draft_key) {
        log::warn!("drafts: the encrypted mirror could not be removed: {err}");
    }
}

/// Whether the engine KEPT the draft despite a put error, so the mirror must
/// survive. The outbox records the op BEFORE the provider call: a provider
/// failure parks the op (kept, waiting for a network), and an applied-but-
/// unrecorded write is on the server (kept). A payload-encode failure happens
/// before the op is enqueued, so the engine kept nothing and the mirror rolls
/// back. Every other arm (a store error, a not-claimable op) is ambiguous and
/// kept conservatively: an orphan mirror is harmless ciphertext, while rolling
/// back a kept draft's mirror would strip its at-rest protection and force
/// resume onto the plaintext engine body.
pub(super) fn engine_kept_draft(err: &ApiError) -> bool {
    !matches!(
        err,
        ApiError::Sync(SyncError::Outbox(detail)) if detail.starts_with("encode draft put")
    )
}

/// What the mirror read answers a resume: the vault words, an absent mirror
/// (fall back to the engine body), or a fail-closed refusal.
pub(super) enum MirrorRead {
    /// The mirror holds the draft: the deserialised words come from the vault.
    Words(Box<Draft>),
    /// No mirror row: the draft was never mirrored, so the engine body stands.
    Absent,
    /// The mirror exists but cannot be decrypted: fail closed, never plaintext.
    Closed,
}

/// Read the mirror for `stored`, keyed by the same `Message-ID` the save wrote
/// and the resume adopts. An app with no store, or a draft carrying no header,
/// reads as absent (the engine-body fallback). A row that will not decrypt or
/// deserialise fails closed.
pub(super) fn read_mirror<P: Provider>(
    app: &App<P>,
    account: &AccountId,
    stored: &Message,
) -> MirrorRead {
    let Some(store) = app.crypto_store() else {
        return MirrorRead::Absent;
    };
    let Some(key) = stored.envelope.message_id.first() else {
        return MirrorRead::Absent;
    };
    match get_encrypted_draft(&store, account.as_str(), key.as_str()) {
        Ok(Some(bytes)) => match serde_json::from_slice::<Draft>(&bytes) {
            Ok(draft) => MirrorRead::Words(Box::new(draft)),
            Err(_) => MirrorRead::Closed,
        },
        Ok(None) => MirrorRead::Absent,
        Err(_) => MirrorRead::Closed,
    }
}

/// One mirrored draft as the composer's resume record: the fields and the
/// words come from the vault, the carried parts as the document's pre-attached
/// rows (metadata only), the same shape the engine-body resume answers.
pub(super) fn resume_from_draft(
    account: &str,
    draft: &Draft,
    parts: &[(engine_api::MessageAttachment, CarriedPart)],
) -> DraftResume {
    DraftResume {
        account: account.to_owned(),
        to: join_emails(&draft.to),
        cc: join_emails(&draft.cc),
        bcc: join_emails(&draft.bcc),
        subject: draft.subject.clone(),
        body_text: draft.text_body.clone(),
        attachments: parts
            .iter()
            .filter_map(|(part, _)| {
                carry::row(
                    part.id().as_u32(),
                    part.file_name(),
                    part.media_type(),
                    part.size(),
                )
            })
            .collect(),
    }
}

/// Joins addresses into the comma-separated text a composer's fields show.
fn join_emails(addresses: &[EmailAddress]) -> String {
    addresses
        .iter()
        .map(|address| address.email.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Publish the fail-closed banner for a mirror that will not decrypt: the
/// error banner alone, no body legs, no plaintext, the reading snapshot's own
/// shape. The resume answers `None` beside it, so no composer opens on a draft
/// the vault cannot read.
pub(super) fn publish_closed_banner<P: Provider>(
    app: &App<P>,
    account: &AccountId,
    stored: &Message,
) {
    log::warn!("drafts: the encrypted draft mirror could not be decrypted");
    app.reading.publish(Some(ReadingSnapshot {
        message: MessageKey {
            account: account.clone(),
            key: stored.id.key().clone(),
        },
        subject: stored.envelope.subject.clone().unwrap_or_default(),
        from_name: String::new(),
        from_address: stored
            .envelope
            .from
            .first()
            .map_or_else(String::new, |from| from.email.clone()),
        sent_at: String::new(),
        html: None,
        plain: None,
        attachments: Vec::new(),
        remote_image_count: 0,
        invitation: None,
        security_banner: Some(SecurityBannerView {
            level: SecurityLevelView::Error,
            text: "Encrypted draft; cannot decrypt.".to_owned(),
        }),
        password_prompt: None,
    }));
}

#[cfg(test)]
#[path = "encrypt_tests.rs"]
mod encrypt_tests;
