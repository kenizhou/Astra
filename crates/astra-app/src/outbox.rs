// SPDX-License-Identifier: GPL-3.0-only
//! Sending: the submit arm through the durable outbox, the outbox family over
//! the engine's queue ops, and the [`Sending`](crate::Surface::Sending) surface
//! that publishes the queue itself.
//!
//! The engine owns the queue and every decision about it: what is due, whether
//! a failure parks or settles, and whether an op can be withdrawn at all. This
//! layer decides only what a person is offered, and turns each refusal into
//! something a log line can say rather than a silent no-op. The op surface is
//! the engine's own: cancel, retry now, and the confirmation pair
//! `confirm_pending_op` answers, which the engine at the pinned revision
//! exposes directly, so nothing here re-derives a queue rule.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content, its
//! recipients, or its subject; counts and ids only.

use std::collections::HashMap;

use astra_composer::{render, ComposerDocument};
use astra_viewmodel::{queued_row, SendingSnapshot};
use engine_api::{
    AccountId, Confirmation, Draft, DraftAttachment, EmailAddress, OpRejection, PendingOpKind,
    Provider, SubmitOutcome,
};

use crate::{
    drafts::{mint_message_id, now_instant, parse_addresses},
    protocol::ComposerBlob,
    reference::QueuedRef,
    App, OutboxIntent,
};

/// The sending state beside the queue itself: what the app adds that the store
/// does not hold.
#[derive(Default)]
pub(crate) struct Outbox {
    /// When each queued send first appeared in this app's read of the store,
    /// keyed by account and op id. The store stamps no arrival instant of its
    /// own, so the app notes the moment it first saw each op; a send already
    /// queued when the app started is unknown and reads as an empty stamp.
    arrived: HashMap<(AccountId, u64), String>,
}

impl<P: Provider> App<P> {
    /// The sending snapshot, for the host's pull after a
    /// [`Surface::Sending`](crate::Surface::Sending) signal: the sends that
    /// have not gone out, across every account, or nothing outstanding.
    #[must_use]
    pub fn sending(&self) -> SendingSnapshot {
        self.sending.get()
    }

    /// Routes one [`OutboxIntent`] to its handler.
    pub(crate) async fn dispatch_outbox(&self, intent: OutboxIntent) {
        match intent {
            OutboxIntent::Show => self.rebuild_sending().await,
            OutboxIntent::Cancel { queued } => self.cancel_queued_send(&queued).await,
            OutboxIntent::SendNow { queued } => self.send_queued_now(&queued).await,
            OutboxIntent::ConfirmSent { queued } => {
                self.confirm_queued_send(&queued, Confirmation::Delivered)
                    .await;
            }
            OutboxIntent::ConfirmNotSent { queued } => {
                self.confirm_queued_send(&queued, Confirmation::NotDelivered)
                    .await;
            }
        }
    }

    /// Renders the composer's document and submits it through the account's
    /// durable outbox, then drains the account so the send and anything else
    /// due happens in this gesture rather than at the next refresh.
    ///
    /// A refusal before the queue (no account, no recipient, a document that
    /// will not build) sends nothing and queues nothing: the honest answer over
    /// a blank message. Whatever the attempt came to, the queue holds it and
    /// the sending snapshot published below is what tells the user.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn submit_rich_mail(
        &self,
        from: AccountId,
        to: String,
        cc: String,
        bcc: String,
        subject: String,
        document: ComposerDocument,
        blobs: Vec<ComposerBlob>,
    ) {
        let Some(handle) = self.account_handle(&from).await else {
            log::warn!("send: the account to send from is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("send: the account has no provider to send through");
            return;
        };
        let Some(draft) = submit_draft(
            &handle.identity,
            &to,
            &cc,
            &bcc,
            &subject,
            &document,
            &blobs,
        ) else {
            log::warn!("send: the message could not be built for sending");
            return;
        };
        let at = self.account_ordinal(&from).await;
        log::info!("send[a{at}]: submitting a message");
        match self.engine.submit_mail(provider, &from, &draft).await {
            Ok(outcome) => log_sent(&outcome, at),
            // The engine records what became of the attempt on the op itself,
            // so the queue answers whether the message is lost, and the
            // snapshot below is where the user reads it.
            Err(err) => log::info!("send[a{at}]: the send did not complete: {err}"),
        }
        self.drain_account(&from).await;
        self.rebuild_sending().await;
        // A delivered send's copy reaches the store at the next sync; the
        // cached lists must not outlive it.
        self.invalidate_list_cache();
    }

    /// Withdraws a queued send so it is never delivered, or dismisses one the
    /// server refused.
    ///
    /// A refusal is logged rather than swallowed, and the row stays on screen:
    /// the refusals that matter say the message may already have gone, which
    /// the snapshot the rebuild publishes keeps in front of the user.
    async fn cancel_queued_send(&self, queued: &QueuedRef) {
        let at = self.account_ordinal(&queued.account).await;
        match self
            .engine
            .cancel_pending_op(&queued.account, queued.op)
            .await
        {
            Ok(None) => {
                log::info!("outbox[a{at}]: a queued send was withdrawn; it will not be sent");
            }
            Ok(Some(refusal)) => {
                log::info!(
                    "outbox[a{at}]: a queued send could not be withdrawn: {}",
                    refusal_words(refusal)
                );
            }
            Err(err) => log::warn!("outbox[a{at}]: withdrawing a queued send failed: {err}"),
        }
        self.rebuild_sending().await;
    }

    /// Attempts a queued send now instead of waiting out its backoff, or sends
    /// again one the server refused, then drains so the attempt happens in
    /// this gesture rather than at a later pass.
    async fn send_queued_now(&self, queued: &QueuedRef) {
        let at = self.account_ordinal(&queued.account).await;
        match self
            .engine
            .retry_pending_op_now(&queued.account, queued.op)
            .await
        {
            Ok(None) => {
                log::info!("outbox[a{at}]: a queued send was asked for now");
                self.drain_account(&queued.account).await;
            }
            Ok(Some(refusal)) => log::info!(
                "outbox[a{at}]: a queued send cannot be sent now: {}",
                refusal_words(refusal)
            ),
            Err(err) => log::warn!("outbox[a{at}]: sending a queued send now failed: {err}"),
        }
        self.rebuild_sending().await;
    }

    /// Records the user's answer to a send whose delivery could not be
    /// confirmed.
    ///
    /// The store refuses an answer to a send that is not waiting for one, so a
    /// row that changed state under the click cannot be sent again through
    /// here. An answer that it did not arrive drains at once, as send now
    /// does: the one way a message that may have been delivered is ever
    /// attempted again.
    async fn confirm_queued_send(&self, queued: &QueuedRef, answer: Confirmation) {
        let at = self.account_ordinal(&queued.account).await;
        match self
            .engine
            .confirm_pending_op(&queued.account, queued.op, answer)
            .await
        {
            Ok(None) if answer == Confirmation::Delivered => log::info!(
                "outbox[a{at}]: the user says a queued send was delivered; it will not be sent again"
            ),
            Ok(None) => {
                log::info!(
                    "outbox[a{at}]: the user says a queued send was not delivered; sending it again"
                );
                self.drain_account(&queued.account).await;
            }
            Ok(Some(refusal)) => log::info!(
                "outbox[a{at}]: an answer for a queued send was refused: {}",
                refusal_words(refusal)
            ),
            Err(err) => log::warn!("outbox[a{at}]: recording the user's answer failed: {err}"),
        }
        self.rebuild_sending().await;
    }

    /// Re-reads every account's outbox and publishes the sending snapshot,
    /// but only when it moved: a gesture that changed nothing says nothing,
    /// and a host that opens the panel pulls the snapshot regardless.
    async fn rebuild_sending(&self) {
        let mut rows = Vec::new();
        for id in self.account_ids().await {
            let queued = self.engine.outbox(&id).await.unwrap_or_default();
            // The stamps are taken under a lock held across no await: each op
            // the read has not seen before is stamped now, and one that has
            // been seen keeps the instant it first appeared.
            let stamps: Vec<String> = {
                let mut state = self.outbox.lock().expect("outbox mutex poisoned");
                queued
                    .iter()
                    .map(|op| {
                        state
                            .arrived
                            .entry((id.clone(), op.id.get()))
                            .or_insert_with(now_instant)
                            .clone()
                    })
                    .collect()
            };
            for (op, stamp) in queued.iter().zip(stamps) {
                if let Some(row) = queued_row(&id, op, stamp) {
                    rows.push(row);
                }
            }
        }
        let snapshot = SendingSnapshot { rows };
        if self.sending.get() != snapshot {
            self.sending.publish(snapshot);
        }
    }

    /// Drains every account's outbox through its first provider.
    ///
    /// A provider failure is not an error: the engine parks each op against
    /// its own retryable class for a later pass, and the queue still owes the
    /// attempt.
    pub(crate) async fn drain_outboxes(&self) {
        for id in self.account_ids().await {
            self.drain_account(&id).await;
        }
    }

    /// One account's drain pass. When a send moved, the queue is in the store
    /// and so is published here rather than waiting for the sync that follows,
    /// which a silent server can hold for as long as it likes.
    async fn drain_account(&self, id: &AccountId) {
        let Some(handle) = self.account_handle(id).await else {
            return;
        };
        let Some(provider) = handle.providers.first() else {
            return;
        };
        let at = self.account_ordinal(id).await;
        match self.engine.drain_outbox(provider, id).await {
            Ok(report) if report.is_idle() => {}
            Ok(report) => {
                log::info!(
                    "outbox[a{at}]: drain attempted {} queued write(s)",
                    report.attempted.len(),
                );
                if report
                    .attempted
                    .iter()
                    .any(|op| op.kind == PendingOpKind::MailSubmit)
                {
                    self.rebuild_sending().await;
                }
            }
            Err(err) => log::warn!("outbox[a{at}]: a drain pass failed: {err}"),
        }
    }
}

/// Logs how a successful submission ended, in the two shapes it can take: the
/// copy filed in Sent, or delivered with the copy unfiled, which is the user's
/// mail gone and their record of it missing, and must be said aloud.
fn log_sent(outcome: &SubmitOutcome, at: usize) {
    match outcome.sent_copy.unfiled_detail() {
        None => log::info!("send[a{at}]: delivered, and the copy is filed in Sent"),
        Some(detail) => {
            log::warn!("send[a{at}]: delivered, but the Sent copy was not filed: {detail}");
        }
    }
}

/// A refusal in words, for the log: product surface language, never the
/// engine's own variant names.
fn refusal_words(refusal: OpRejection) -> &'static str {
    match refusal {
        OpRejection::Unknown => "it is no longer queued",
        OpRejection::Settled => "it has already gone",
        OpRejection::InFlight => "it is being sent right now",
        OpRejection::AwaitingConfirmation => "it may already have been delivered",
        OpRejection::NotAwaitingConfirmation => "it is not waiting for an answer",
    }
}

/// Builds the draft a send submits: the document through the composer's
/// deterministic renderer (so a sent message is byte-identical to a draft
/// stored from the same document), a fresh `Message-ID` (the op is idempotent
/// by it), and every attachment's bytes resolved from the host's blobs.
///
/// `None` when the message must not go: no resolvable recipient, a document
/// that will not render, or an attachment whose bytes the host did not
/// resolve. A message silently missing its files is worse than one not sent.
fn submit_draft(
    identity: &EmailAddress,
    to: &str,
    cc: &str,
    bcc: &str,
    subject: &str,
    document: &ComposerDocument,
    blobs: &[ComposerBlob],
) -> Option<Draft> {
    let to = parse_addresses(to);
    let cc = parse_addresses(cc);
    let bcc = parse_addresses(bcc);
    if to.is_empty() && cc.is_empty() && bcc.is_empty() {
        return None;
    }
    let output = render(document).ok()?;
    let message_id = mint_message_id()?;
    let mut draft = Draft::new(
        message_id,
        identity.clone(),
        to,
        subject.to_owned(),
        output.plain_text,
    )
    .with_cc(cc)
    .with_bcc(bcc);
    draft.html_body = Some(output.html);
    let mut bytes: HashMap<&str, &[u8]> = blobs
        .iter()
        .map(|blob| (blob.handle.as_str(), blob.bytes.as_slice()))
        .collect();
    for attachment in output.inline_attachments.iter().chain(&output.attachments) {
        let content = attachment
            .blob
            .as_ref()
            .and_then(|handle| bytes.remove(handle.as_str()))?
            .to_vec();
        draft = match &attachment.cid {
            Some(cid) => {
                let content_id = engine_api::ContentIdHeader::new(cid.as_str()).ok()?;
                draft.with_attachment(DraftAttachment::inline(
                    attachment.file_name.clone(),
                    attachment.media_type.clone(),
                    content_id,
                    content,
                ))
            }
            None => draft.with_attachment(DraftAttachment::attachment(
                attachment.file_name.clone(),
                attachment.media_type.clone(),
                content,
            )),
        };
    }
    Some(draft)
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod tests;
