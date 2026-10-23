// SPDX-License-Identifier: GPL-3.0-only
//! Sending the composer's document: the three submit arms over the durable
//! outbox. The plain submit builds a draft and queues it; the reply and the
//! forward thread it under the original each answers, then queue it through
//! the same tail ([`App::submit_and_drain`]), so every send a submit starts
//! is owed by the engine's queue whatever the network and the app's own
//! lifetime do.
//!
//! The threading is resolved at submit, the way mail-calendar's arms do: the
//! original is re-read from the store, and its `References` chain followed by
//! its own `Message-ID` becomes the outgoing chain, a reply anchoring it with
//! an `In-Reply-To` and a forward carrying the chain alone. Astra's intents
//! carry no reference to the original (the reply knows only the header, the
//! forward nothing), so the composition the compose family published names
//! it: the submit arms re-resolve that message through the reading resolve
//! path. A reply whose original cannot be re-resolved still sends, threading
//! by its header alone; a forward cannot, because the original is the only
//! source of the conversation it belongs to.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content, its
//! recipients, or its subject; counts and ids only.

use std::collections::HashMap;

use astra_composer::{render, ComposerDocument, DraftBlobHandle};
use engine_api::{
    AccountId, AttachmentPartId, Draft, DraftAttachment, EmailAddress, Message, MessageIdHeader,
    Provider, SubmitOutcome,
};

use crate::{
    compose_request::carry,
    crypto::CryptoFlags,
    drafts::{mint_message_id, parse_addresses},
    gate::{GateComposition, GateVerdict},
    protocol::ComposerBlob,
    reading::resolve,
    App,
};

/// The gate's view of one built draft: the envelope and both rendered legs.
fn gate_composition_of(from: &AccountId, draft: &Draft) -> GateComposition {
    GateComposition {
        account: from.clone(),
        from: draft.from.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        plain: draft.text_body.clone(),
        html: draft.html_body.clone().unwrap_or_default(),
    }
}

impl<P: Provider> App<P> {
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
        crypto: CryptoFlags,
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
        self.gate_and_submit(&from, provider, draft, crypto).await;
    }

    /// Sends the composer's document as a reply: the same path as the plain
    /// submit, with the draft answering `in_reply_to` under the original's
    /// whole chain ([`App::reply_references`]).
    ///
    /// A refusal before the queue is the plain submit's: no account, no
    /// recipient, a document that will not build. The threading degrades
    /// rather than refuses: a reply whose original cannot be re-resolved
    /// still sends, under its header alone.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn submit_rich_reply(
        &self,
        from: AccountId,
        to: String,
        cc: String,
        in_reply_to: MessageIdHeader,
        subject: String,
        document: ComposerDocument,
        blobs: Vec<ComposerBlob>,
        crypto: CryptoFlags,
    ) {
        let Some(handle) = self.account_handle(&from).await else {
            log::warn!("send: the account to send from is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("send: the account has no provider to send through");
            return;
        };
        let references = self.reply_references(&in_reply_to).await;
        let Some(draft) = submit_draft(&handle.identity, &to, &cc, "", &subject, &document, &blobs)
        else {
            log::warn!("send: the message could not be built for sending");
            return;
        };
        let draft = draft.in_reply_to(in_reply_to, references);
        self.gate_and_submit(&from, provider, draft, crypto).await;
    }

    /// Sends the composer's document as a forward: the same path as the plain
    /// submit, with the draft carrying the original's whole chain and no
    /// `In-Reply-To`, because a forward continues a conversation without
    /// answering a message.
    ///
    /// A forward whose original will not re-resolve sends nothing: it has no
    /// header of its own, so the original is the only source of the
    /// conversation it belongs to, and a forward without it is a new message
    /// pretending to be one. The carried attachments resolve off the same
    /// re-read: a standing row whose part will not come back refuses the
    /// send, the same discipline a host blob that is missing serves.
    pub(crate) async fn submit_rich_forward(
        &self,
        from: AccountId,
        to: String,
        subject: String,
        document: ComposerDocument,
        blobs: Vec<ComposerBlob>,
        crypto: CryptoFlags,
    ) {
        let Some(handle) = self.account_handle(&from).await else {
            log::warn!("send: the account to send from is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("send: the account has no provider to send through");
            return;
        };
        let Some(original) = self.compose_original().await else {
            log::warn!("send: the original to forward is no longer in the store");
            return;
        };
        let carry = self
            .compose_carry
            .lock()
            .expect("compose-carry mutex poisoned")
            .clone();
        let mut blobs = blobs;
        for id in carry::standing(&carry, &document) {
            match self
                .engine
                .message_attachment(provider, &from, &original, AttachmentPartId::new(id))
                .await
            {
                Ok(Some(content)) => blobs.push(ComposerBlob {
                    handle: DraftBlobHandle::new(carry::handle_of(id))
                        .expect("a minted carry handle is never empty"),
                    bytes: content.into_bytes(),
                }),
                Ok(None) => {
                    log::warn!("send: the original carries no such attachment to forward");
                    return;
                }
                Err(err) => {
                    log::warn!("send: the carried attachment could not be fetched: {err}");
                    return;
                }
            }
        }
        let Some(draft) = submit_draft(&handle.identity, &to, "", "", &subject, &document, &blobs)
        else {
            log::warn!("send: the message could not be built for sending");
            return;
        };
        let draft = draft.with_references(original_chain(&original));
        self.gate_and_submit(&from, provider, draft, crypto).await;
    }

    /// Submits `draft` through the account's durable outbox and drains the
    /// account, the tail every submit arm shares: the send and anything else
    /// due happens in this gesture rather than at the next refresh, the
    /// queue holds whatever the attempt came to, and the sending snapshot
    /// the rebuild publishes is what tells the user.
    pub(crate) async fn submit_and_drain(
        &self,
        from: &AccountId,
        provider: &P,
        draft: Draft,
        crypto: CryptoFlags,
    ) {
        // The crypto flags stamp onto the engine's draft: the account's
        // installed MIME wrapper wraps: or refuses: inside the engine's own
        // outbox, and a plain account's send is untouched (no wrapper, the
        // flags inert). Stamped after the gate, so what the gate vetted is
        // exactly what the wrapper wraps.
        let (sign, encrypt) = crypto.resolved();
        let draft = draft.with_crypto(sign, encrypt);
        let at = self.account_ordinal(from).await;
        log::info!("send[a{at}]: submitting a message");
        match self.engine.submit_mail(provider, from, &draft).await {
            Ok(outcome) => log_sent(&outcome, at),
            // The engine records what became of the attempt on the op itself,
            // so the queue answers whether the message is lost, and the
            // snapshot below is where the user reads it.
            Err(err) => log::info!("send[a{at}]: the send did not complete: {err}"),
        }
        self.drain_account(from).await;
        self.rebuild_sending().await;
        // A delivered send's copy reaches the store at the next sync; the
        // cached lists must not outlive it.
        self.invalidate_list_cache();
    }

    /// The gate seam every submit arm runs instead of queueing directly: the
    /// transform rewrites the rendered legs first (failing closed), then the
    /// gate vets the composition (the held row stands `Checking` from the
    /// first second), and only an allow queues. With neither port installed
    /// this is exactly [`App::submit_and_drain`], which is the zero-cost
    /// default.
    pub(crate) async fn gate_and_submit(
        &self,
        from: &AccountId,
        provider: &P,
        mut draft: Draft,
        crypto: CryptoFlags,
    ) {
        if self.send_gate.is_none() && self.compose_transform.is_none() {
            self.submit_and_drain(from, provider, draft, crypto).await;
            return;
        }
        let at = self.account_ordinal(from).await;
        if let Some(transform) = self.compose_transform.clone() {
            match transform.transform(gate_composition_of(from, &draft)).await {
                Ok(rewritten) => {
                    draft.text_body = rewritten.plain;
                    draft.html_body = Some(rewritten.html);
                }
                Err(failure) => {
                    log::info!("send[a{at}]: a transform held the send: {}", failure.reason);
                    self.hold_send(
                        from.clone(),
                        draft,
                        crypto,
                        failure.plugin,
                        failure.reason,
                        astra_viewmodel::HeldState::Blocked,
                    )
                    .await;
                    return;
                }
            }
        }
        let Some(gate) = self.send_gate.clone() else {
            self.submit_and_drain(from, provider, draft, crypto).await;
            return;
        };
        let composition = gate_composition_of(from, &draft);
        let hold = self
            .hold_send(
                from.clone(),
                draft,
                crypto,
                String::new(),
                String::new(),
                astra_viewmodel::HeldState::Checking,
            )
            .await;
        match gate.check(&composition).await {
            GateVerdict::Allow => {
                if let Some(entry) = self.take_hold(&hold).await {
                    self.submit_and_drain(&entry.from.clone(), provider, entry.draft, crypto)
                        .await;
                }
            }
            GateVerdict::PromptUser { plugin, reason } => {
                self.settle_hold(
                    &hold,
                    plugin,
                    reason,
                    astra_viewmodel::HeldState::NeedsAnswer,
                )
                .await;
            }
            GateVerdict::Block { plugin, reason } => {
                log::info!("send[a{at}]: the gate held the send: {reason}");
                self.settle_hold(&hold, plugin, reason, astra_viewmodel::HeldState::Blocked)
                    .await;
            }
        }
    }

    /// The original the pending compose request was computed from,
    /// re-resolved through the reading resolve path at the moment of the
    /// send: the same re-read mail-calendar's submit arms make of the message
    /// their intent names. `None` when no request is pending or the message
    /// has left the store.
    async fn compose_original(&self) -> Option<Message> {
        let source = self
            .compose_source
            .lock()
            .expect("compose-source mutex poisoned")
            .clone()?;
        resolve(&self.engine, &source).await
    }

    /// The `References` chain a reply carries: the original's own chain
    /// followed by the original itself, oldest first. The original is
    /// re-resolved because the intent carries only the header; when it
    /// resolves and its own `Message-ID` is the header the reply answers,
    /// the whole chain rides. Otherwise the reply threads by the header
    /// alone, the parent itself the chain: still correct mail, and never
    /// another conversation's chain under a stale parent.
    async fn reply_references(&self, parent: &MessageIdHeader) -> Vec<MessageIdHeader> {
        match self.compose_original().await {
            Some(original) if original.envelope.message_id.first() == Some(parent) => {
                original_chain(&original)
            }
            _ => vec![parent.clone()],
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

/// The chain `original` threads under, mail-calendar's computation: its
/// `References` followed by its own `Message-ID`, oldest first. Empty when
/// the original arrived with no id of its own, which is valid mail.
fn original_chain(original: &Message) -> Vec<MessageIdHeader> {
    let mut references = original.envelope.references.clone();
    if let Some(parent) = original.envelope.message_id.first() {
        references.push(parent.clone());
    }
    references
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
#[path = "submit_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "submit_gate_tests.rs"]
mod gate_tests;
