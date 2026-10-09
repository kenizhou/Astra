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

use engine_api::{AccountId, EmailAddress, Message, Provider};

use super::{Composition, CompositionId};
use crate::App;

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
        let resumed = DraftResume {
            account: account.as_str().to_owned(),
            to: join_emails(&newest.envelope.to),
            cc: join_emails(&newest.envelope.cc),
            bcc: join_emails(&newest.envelope.bcc),
            subject: newest.envelope.subject.clone().unwrap_or_default(),
            body_text: body.plain().unwrap_or_default().to_owned(),
        };
        if !self.adopt_stored_draft(&composition, account, &newest) {
            return None;
        }
        Some(resumed)
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
