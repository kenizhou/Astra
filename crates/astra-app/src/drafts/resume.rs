// SPDX-License-Identifier: GPL-3.0-only
//! Opening the account's latest stored draft back into a composer.
//!
//! A child of [`super`], whose `impl App` block this one continues.
//!
//! **What makes this more than a read.** The composer it opens will save, and that save
//! supersedes the copy it came from, so everything the composer does not open with is
//! something the next save takes out of the user's Drafts folder. That is why the
//! composition adopts the stored draft's key here: a resumed composer that saved with
//! nothing to replace would leave the user two drafts and no way to tell which one they
//! are editing.

use engine_api::{AccountId, AttachmentPartId, EmailAddress, Message, Provider};

use super::{CarriedPart, Composition, CompositionId};
use crate::{compose_request::carry, reading::resolve, reference::MessageRef, App};

/// A stored draft, as the composer that resumes it needs to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftResume {
    /// The account whose Drafts folder the draft was in, and which its saves go back to.
    pub account: String,
    /// The `To` field, comma-joined.
    pub to: String,
    /// The `Cc` field, comma-joined.
    pub cc: String,
    /// The `Bcc` field, comma-joined. Only what the stored copy carries: a `Bcc` is not
    /// in the bytes most transports hand back, so a draft saved elsewhere usually
    /// resumes without one.
    pub bcc: String,
    /// The subject.
    pub subject: String,
    /// The body, as plain text: what the composer can open without a second markup
    /// parser. The files the draft carries join the resume with the attachments slice.
    pub body_text: String,
    /// The files the stored draft carries, as the document's pre-attached removable
    /// rows: the forward's own carry shape, a blob handle naming the part behind it
    /// and metadata only, never bytes and never a `data:` URI. The submit resolves
    /// the standing rows' bytes core-side.
    pub attachments: Vec<astra_composer::DraftAttachment>,
}

impl<P: Provider> App<P> {
    /// Opens the account's latest stored draft into `composition`, so the composer that
    /// holds it saves over that copy rather than beside it.
    ///
    /// `composition` is the host's, minted for the composer it is about to open, exactly
    /// as for a fresh message: the core never mints one. Resuming into an id that is
    /// already open replaces what was known about it, which is what a host reusing an id
    /// means by it.
    ///
    /// `None` when the account holds no draft, its body cannot be read, or the account
    /// is not configured; nothing is adopted in any of those cases, because a
    /// composition that opened without the draft's content must not be one whose next
    /// save replaces it.
    pub async fn resume_latest_draft(
        &self,
        account: &AccountId,
        composition: CompositionId,
    ) -> Option<DraftResume> {
        let handle = self.account_handle(account).await?;
        let provider = handle.providers.first()?;
        let mut stored: Vec<Message> = self
            .engine
            .messages(account)
            .await
            .ok()?
            .into_iter()
            .filter(Message::is_draft)
            .collect();
        stored.sort_by_key(|message| message.received_at.or(message.sent_at));
        let newest = stored.pop()?;
        let body = self
            .engine
            .message_body(provider, account, &newest)
            .await
            .ok()?;
        let parts = self.read_carried_parts(provider, account, &newest).await;
        let resumed = resume_record(
            account.as_str(),
            &newest,
            body.plain().unwrap_or_default(),
            &parts,
        );
        if !self.adopt_stored_draft(&composition, account, &newest) {
            return None;
        }
        self.park_carried(
            &composition,
            parts.into_iter().map(|(_, parked)| parked).collect(),
        );
        Some(resumed)
    }

    /// Opens the Drafts row's named copy into `composition`: the same adoption and
    /// the same three refusal gates as the latest-entry resume, addressed by the
    /// row's own account and key rather than by recency, so a folder of drafts
    /// reopens the one the user clicked. The files the stored copy carries
    /// pre-attach as removable rows and park their bytes on the composition.
    ///
    /// `None` when the key resolves to nothing, the message is not a draft, or its
    /// body cannot be read; nothing is adopted in any of those cases, because a
    /// composition that opened without the draft's content must not be one whose
    /// next save replaces it.
    pub async fn resume_draft_by_key(
        &self,
        message: &MessageRef,
        composition: CompositionId,
    ) -> Option<DraftResume> {
        let handle = self.account_handle(&message.account).await?;
        let provider = handle.providers.first()?;
        let stored = resolve(&self.engine, message).await?;
        if !stored.is_draft() {
            log::warn!("drafts: the key names a message that is not a draft");
            return None;
        }
        let body = self
            .engine
            .message_body(provider, &message.account, &stored)
            .await
            .ok()?;
        let parts = self
            .read_carried_parts(provider, &message.account, &stored)
            .await;
        let resumed = resume_record(
            message.account.as_str(),
            &stored,
            body.plain().unwrap_or_default(),
            &parts,
        );
        if !self.adopt_stored_draft(&composition, &message.account, &stored) {
            return None;
        }
        self.park_carried(
            &composition,
            parts.into_iter().map(|(_, parked)| parked).collect(),
        );
        Some(resumed)
    }

    /// The files `stored` carries, read once with their bytes: the rows the resume
    /// answers with beside the parts it parks on the composition. A part whose
    /// bytes will not read parks nothing and shows no row, the forward's own rule:
    /// there is no row to show for a part that is not there.
    async fn read_carried_parts(
        &self,
        provider: &P,
        account: &AccountId,
        stored: &Message,
    ) -> Vec<(engine_api::MessageAttachment, CarriedPart)> {
        let parts = self
            .engine
            .message_attachments(provider, account, stored)
            .await
            .unwrap_or_else(|err| {
                log::warn!("drafts: the stored draft's parts did not read: {err}");
                Vec::new()
            });
        let mut carried = Vec::new();
        for part in parts {
            let id = part.id();
            match self
                .engine
                .message_attachment(
                    provider,
                    account,
                    stored,
                    AttachmentPartId::new(id.as_u32()),
                )
                .await
            {
                Ok(Some(content)) => {
                    let parked = CarriedPart {
                        id: id.as_u32(),
                        file_name: part.file_name().to_owned(),
                        media_type: part.media_type().to_owned(),
                        bytes: content.into_bytes(),
                    };
                    carried.push((part, parked));
                }
                Ok(None) => {
                    log::warn!("drafts: the stored draft names a part it does not carry");
                }
                Err(err) => {
                    log::warn!("drafts: a stored part could not be read: {err}");
                }
            }
        }
        carried
    }

    /// Joins `composition` to the copy already on the server: its key, so the first save
    /// supersedes it and a discard removes it, and its `Message-ID`, so every save of
    /// this composition keeps naming the one message.
    ///
    /// A draft written elsewhere may carry no `Message-ID` at all, and a fresh one then
    /// serves as well: what the header has to be is the *same* on every save of this
    /// composition, which is what the engine leases a queued save's resource on. It is
    /// the key, not the header, that names the copy being replaced.
    ///
    /// No digest is recorded: the body the composer opens with is text derived from the
    /// stored draft, and re-rendering it produces a different message than the one on
    /// the server, so the first save after a resume always writes.
    ///
    /// Answers whether the composition was adopted: `false` when no header could be
    /// named, in which case the caller reports no resume at all rather than handing the
    /// composer a copy its saves will not supersede.
    fn adopt_stored_draft(
        &self,
        composition: &CompositionId,
        account: &AccountId,
        stored: &Message,
    ) -> bool {
        let message_id = stored
            .envelope
            .message_id
            .first()
            .cloned()
            .or_else(super::mint_message_id);
        let Some(message_id) = message_id else {
            // Neither the draft's own header nor a new one: nothing to key the saves on,
            // so the composition is left unknown and the stored copy where it is.
            log::warn!("drafts: the resumed draft has no id to save under");
            return false;
        };
        let mut drafts = self.drafts.lock().expect("drafts mutex poisoned");
        // A composer that has just opened has saved nothing, whatever a composition of
        // this name did before it: the hint is per composer.
        if drafts
            .open
            .insert(
                composition.clone(),
                Composition {
                    account: account.clone(),
                    message_id,
                    key: Some(stored.id.key().clone()),
                    saved: None,
                    held: None,
                    carry: Vec::new(),
                },
            )
            .is_some()
        {
            log::warn!("drafts: a composition id was resumed over while open");
        }
        if self
            .draft_status
            .get()
            .is_some_and(|status| status.composition == composition.as_str())
        {
            self.draft_status.publish(None);
        }
        true
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

/// One stored draft as the composer's resume record: the fields to hydrate from
/// and the carried parts as the document's pre-attached rows, metadata only.
fn resume_record(
    account: &str,
    stored: &Message,
    body_plain: &str,
    parts: &[(engine_api::MessageAttachment, CarriedPart)],
) -> DraftResume {
    DraftResume {
        account: account.to_owned(),
        to: join_emails(&stored.envelope.to),
        cc: join_emails(&stored.envelope.cc),
        bcc: join_emails(&stored.envelope.bcc),
        subject: stored.envelope.subject.clone().unwrap_or_default(),
        body_text: body_plain.to_owned(),
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

#[cfg(test)]
#[path = "resume_by_key_tests.rs"]
mod by_key_tests;
