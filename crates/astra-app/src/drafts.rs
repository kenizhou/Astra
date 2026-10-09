// SPDX-License-Identifier: GPL-3.0-only
//! Keeping the message being composed on the server: saving one, discarding one, and
//! the per-composition record that joins the two. Resuming a stored draft lives in
//! [`resume`], the idle autosave in [`autosave`], the draft builder and its digest in
//! [`build`].
//!
//! Two rules decide almost everything here, both mail-calendar's. The stored copy is
//! named by the **key the provider answered with**, because three of the four adapters
//! move it on a re-save. And every save of one composition carries the same
//! `Message-ID`: that is what the engine keys the queued op's resource on, so a fresh
//! one per save would leave each save queued *beside* the last rather than superseding
//! it.
//!
//! **Privacy.** A draft is the user's mail. Nothing here logs its content; counts and
//! ids only.

use std::{collections::HashMap, sync::Arc, time::Duration};

use astra_composer::ComposerDocument;
use astra_viewmodel::{DraftSaveState, DraftStatusSnapshot};
use engine_api::{
    AccountId, DraftPut, DrainOutcome, MessageIdHeader, OpRejection, PendingOpId, PendingOpKind,
    Provider, ProviderKey,
};

use crate::{App, CompositionId, DraftsIntent};

mod autosave;
mod build;
mod resume;

use build::{build_draft, digest, mint_message_id, now_instant};
pub use resume::DraftResume;

/// How long a composer sits untouched before its draft is saved.
///
/// The core's constant rather than each client's, so the surfaces cannot disagree about
/// it. A client restarts the interval with every save it dispatches: the trigger is the
/// pause, not the clock, so someone still typing is never made to wait on an upload.
pub const DRAFT_AUTOSAVE_IDLE: Duration = Duration::from_secs(30);

/// What one composer last handed over, held so the verbs that save without a fresh
/// dispatch (an idle fire, a close) store the composer's own words and not a blank.
#[derive(Clone)]
pub(super) struct HeldSave {
    /// The composer's From account.
    pub(super) from: AccountId,
    /// The `To` recipients, comma-separated; may be empty.
    pub(super) to: String,
    /// The `Cc` recipients, comma-separated; may be empty.
    pub(super) cc: String,
    /// The `Bcc` recipients, comma-separated; may be empty.
    pub(super) bcc: String,
    /// The subject; may be empty.
    pub(super) subject: String,
    /// The shared composer document.
    pub(super) document: ComposerDocument,
}

/// What the core knows about one composition, for as long as its composer is open.
pub(super) struct Composition {
    /// The account whose Drafts folder holds it. Fixed by the first save: the stored
    /// copy lives in one account's folder, and moving it later would leave that copy
    /// behind.
    pub(super) account: AccountId,
    /// The header every save of this composition carries, minted once so the engine can
    /// supersede a queued save rather than queue beside it.
    pub(super) message_id: MessageIdHeader,
    /// What the server stores the draft under **now**, or `None` until a save has
    /// actually reached one. Handed to the next save as the copy it replaces.
    pub(super) key: Option<ProviderKey>,
    /// A digest of what the last save put on the server, so an unchanged draft costs no
    /// write. Compared only within this process, which is all [`DefaultHasher`]
    /// promises.
    pub(super) saved: Option<u64>,
    /// The composer's current content, as its last Save carried it.
    pub(super) held: Option<HeldSave>,
}

/// The drafts state: the open compositions and their per-composition facts.
#[derive(Default)]
pub(crate) struct Drafts {
    pub(super) open: HashMap<CompositionId, Composition>,
    /// The armed autosaves, one per composition at most. Their driver and shape live in
    /// [`autosave`].
    pub(super) armed: HashMap<CompositionId, autosave::Armed>,
    /// Distinguishes one arming of the autosave from the next.
    pub(super) arm_seq: u64,
}

impl<P: Provider> App<P> {
    /// Routes one [`DraftsIntent`] to its handler.
    pub(crate) async fn dispatch_drafts(&self, intent: DraftsIntent) {
        match intent {
            DraftsIntent::Save {
                composition,
                from,
                to,
                cc,
                bcc,
                subject,
                document,
                then_close,
            } => {
                let held = HeldSave {
                    from,
                    to,
                    cc,
                    bcc,
                    subject,
                    document,
                };
                self.save_draft(&composition, &held).await;
                if then_close {
                    self.forget(&composition);
                } else {
                    self.arm_autosave(&composition);
                }
            }
            DraftsIntent::Discard { composition } => self.discard_draft(&composition).await,
            DraftsIntent::Close { composition } => {
                self.close_composition(&composition).await;
            }
        }
    }

    /// The draft-status snapshot, for the host's pull after a
    /// [`Surface::DraftStatus`](crate::Surface::DraftStatus) signal: the composition
    /// whose save moved and how it ended, or `None` when no composition has a save to
    /// report.
    #[must_use]
    pub fn draft_status(&self) -> Option<DraftStatusSnapshot> {
        self.draft_status.get()
    }

    /// Stores the composer's content in the account's Drafts folder, replacing what
    /// this composition's previous save left there: the dispatch path, which takes the
    /// save gate for the length of the save. One save at a time, because the second of
    /// two overlapping saves must read the stored key only after the first has recorded
    /// what it stored.
    async fn save_draft(&self, composition: &CompositionId, held: &HeldSave) {
        let gate = Arc::clone(&self.save_gate);
        let _saving = gate.lock().await;
        self.save_draft_gated(composition, held).await;
    }

    /// [`save_draft`](Self::save_draft) for a caller already holding the save gate: the
    /// close and the idle fire, which must not let a forget slip in between their steps.
    async fn save_draft_gated(&self, composition: &CompositionId, held: &HeldSave) {
        // An account already carrying this draft wins over the composer's dropdown: see
        // [`Composition::account`].
        let account = self
            .composition_account(composition)
            .unwrap_or_else(|| held.from.clone());
        let Some(message_id) = self.record_held(composition, &account, held) else {
            log::warn!("drafts: a Message-ID could not be minted for the draft");
            return;
        };
        // The account handle is cloned out so the guard is released before the provider
        // round-trip; an account the app no longer holds cannot be saved to.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("drafts: the account to save to is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("drafts: the account has no provider to save a draft through");
            return;
        };
        let Some(draft) = build_draft(&message_id, &handle.identity, held) else {
            log::warn!("drafts: the draft could not be rendered to store");
            return;
        };
        let digest = digest(&draft);
        let parked = self.parked_save(&account, &message_id).await;
        // Nothing changed since the last landed save and nothing is on its way, so
        // there is nothing to write; still "saved", because what the user asked to keep
        // is on the server. A parked save with different words always wins over this
        // answer, the ordinary undo-on-a-train: the queued op is about to replace the
        // server's copy with the version the user moved away from.
        if parked.is_none() && self.already_saved(composition, digest) {
            self.set_status_saved(composition);
            return;
        }
        self.set_status_saving(composition);
        // The same words are already parked in the outbox waiting for a network: settle
        // that copy rather than queue a second beside it. The engine retries a parked
        // write on its own backoff clock; a save arriving now (the composer's quiet
        // period, or its leaving) clears the backoff and drains, which is the engine's
        // pair for exactly this. Different words fall through to the put below, which
        // supersedes the parked copy the way it supersedes a stored one.
        if let Some((op, _)) = parked.filter(|(_, parked_digest)| *parked_digest == digest) {
            self.settle_parked_save(composition, &account, provider, op, digest)
                .await;
            return;
        }
        let replacing = self.composition_key(composition);
        match self
            .engine
            .put_draft(provider, &account, &draft, replacing.as_ref())
            .await
        {
            Ok(outcome) => {
                log::info!("drafts: the draft is saved on the server");
                self.record_save(composition, outcome.key, digest);
                self.set_status_saved(composition);
                // The store does not hold the copy until the Drafts folder's next sync
                // presents it, so the cached lists are dropped rather than rebuilt.
                self.invalidate_list_cache();
            }
            // The engine parks a retryable failure in the outbox, so the words are not
            // lost: still on their way, which is what the hint says while the next quiet
            // period settles it. The queued-versus-failed split arrives with the sending
            // slice's richer status.
            Err(err) => {
                log::info!("drafts: the save is waiting for the server: {err}");
            }
        }
    }

    /// Settles `op`, this composition's parked save: clears the retry backoff and drains
    /// the outbox, then adopts the key the drain resolved. The drain report is the only
    /// place that key is ever named, and the next save must supersede the copy it names
    /// rather than store a second beside it.
    async fn settle_parked_save(
        &self,
        composition: &CompositionId,
        account: &AccountId,
        provider: &P,
        op: PendingOpId,
        digest: u64,
    ) {
        match self.engine.retry_pending_op_now(account, op).await {
            Ok(None) => {}
            // Said in words rather than the rejection's own name: a log line is product
            // surface and carries no internal jargon. No drain happens, so no attempt
            // is logged either: nothing was tried.
            Ok(Some(refusal)) => {
                let reason = match refusal {
                    OpRejection::Unknown | OpRejection::NotAwaitingConfirmation => {
                        "it is no longer queued"
                    }
                    OpRejection::Settled => "it has already been stored",
                    OpRejection::InFlight => "it is being stored right now",
                    OpRejection::AwaitingConfirmation => "it may already have been stored",
                };
                log::info!("drafts: the parked save cannot be retried yet: {reason}");
                return;
            }
            Err(err) => {
                log::info!("drafts: the parked save could not be read back: {err}");
                return;
            }
        }
        let report = match self.engine.drain_outbox(provider, account).await {
            Ok(report) => report,
            Err(err) => {
                log::info!("drafts: the parked save did not reach the server: {err}");
                return;
            }
        };
        let Some(drained) = report.attempted.iter().find(|drained| drained.id == op) else {
            return;
        };
        match (&drained.outcome, &drained.provider_key) {
            (DrainOutcome::Succeeded, Some(key)) => {
                log::info!("drafts: the parked save landed through the outbox");
                self.record_save(composition, key.clone(), digest);
                self.set_status_saved(composition);
                self.invalidate_list_cache();
            }
            // Still parked, or landed without a key to name: the words stay on their
            // way and the hint stays "saving", which is the honest report.
            _ => {
                log::info!("drafts: the parked save is still waiting for the server");
            }
        }
    }

    /// Removes this composition's stored draft and forgets the composition. The forget
    /// and the removal run under the save gate, so a deadline fire that has taken its
    /// arm as the discard lands cannot begin a save in between and re-store what the
    /// user threw away.
    async fn discard_draft(&self, composition: &CompositionId) {
        let gate = Arc::clone(&self.save_gate);
        let _discarding = gate.lock().await;
        let Some(record) = self.forget(composition) else {
            // An unknown composition or one never saved: nothing is on the server, so
            // discarding it is already done.
            return;
        };
        // A save still waiting for a network would otherwise store the discarded message
        // after the composer is gone, leaving a draft nothing here can name any more.
        if let Some((op, _)) = self.parked_save(&record.account, &record.message_id).await {
            match self.engine.cancel_pending_op(&record.account, op).await {
                Ok(_) => log::info!("drafts: a queued save was withdrawn before it went out"),
                Err(err) => {
                    log::warn!("drafts: withdrawing the queued save failed: {err}");
                }
            }
        }
        let Some(key) = record.key else {
            return;
        };
        let Some(handle) = self.account_handle(&record.account).await else {
            return;
        };
        let Some(provider) = handle.providers.first() else {
            return;
        };
        match self
            .engine
            .delete_draft(provider, &record.account, &key)
            .await
        {
            Ok(_) => {
                log::info!("drafts: the stored draft was removed");
                self.invalidate_list_cache();
            }
            // Queued, like any other write the outbox holds: the copy goes when the
            // network does. Nothing to tell the composer, which has closed.
            Err(err) => log::info!("drafts: the discard did not reach the server yet: {err}"),
        }
    }

    /// Saves what the composition last handed over, then forgets it: the host's `Close`
    /// for a composer that is dismissed without sending, and the draft stays in Drafts
    /// where the user will find it. Both steps run under the save gate, so a deadline
    /// fire that has taken its arm as the close lands queues behind the close and finds
    /// no composition left, rather than re-storing under a new record.
    async fn close_composition(&self, composition: &CompositionId) {
        let gate = Arc::clone(&self.save_gate);
        let _closing = gate.lock().await;
        let held = self
            .drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .and_then(|record| record.held.clone());
        if let Some(held) = held {
            self.save_draft_gated(composition, &held).await;
        }
        self.forget(composition);
    }

    /// Records `held` as the composition's current content, opening the composition's
    /// record when this is its first save, and answers the header every save of it
    /// carries. `None` only when a header could not be minted for a fresh record, which
    /// the caller treats as any other build failure.
    fn record_held(
        &self,
        composition: &CompositionId,
        account: &AccountId,
        held: &HeldSave,
    ) -> Option<MessageIdHeader> {
        let mut drafts = self.drafts.lock().expect("drafts mutex poisoned");
        if let Some(record) = drafts.open.get_mut(composition) {
            record.held = Some(held.clone());
            return Some(record.message_id.clone());
        }
        let message_id = mint_message_id()?;
        drafts.open.insert(
            composition.clone(),
            Composition {
                account: account.clone(),
                message_id: message_id.clone(),
                key: None,
                saved: None,
                held: Some(held.clone()),
            },
        );
        Some(message_id)
    }

    /// The account a composition is already stored under, if it has been saved.
    fn composition_account(&self, composition: &CompositionId) -> Option<AccountId> {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .map(|record| record.account.clone())
    }

    /// The header every save of this composition carries, or `None` when it is not
    /// open: the fire's proof that it is still the composition its arm was taken under.
    fn composition_message_id(&self, composition: &CompositionId) -> Option<MessageIdHeader> {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .map(|record| record.message_id.clone())
    }

    /// What the server stores this composition under now.
    fn composition_key(&self, composition: &CompositionId) -> Option<ProviderKey> {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .and_then(|record| record.key.clone())
    }

    /// Whether the last save put exactly this content on the server.
    fn already_saved(&self, composition: &CompositionId, digest: u64) -> bool {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .is_some_and(|record| record.saved == Some(digest))
    }

    /// Records what the save stored, and under which key.
    fn record_save(&self, composition: &CompositionId, key: ProviderKey, digest: u64) {
        if let Some(record) = self
            .drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get_mut(composition)
        {
            record.key = Some(key);
            record.saved = Some(digest);
        }
    }

    /// The parked save of this composition, if the outbox still holds one, with a digest
    /// of the words it carries.
    ///
    /// Matched on the `Message-ID`, which every save of one composition shares, so this
    /// finds the op whatever text it carries; the digest says whether those words are
    /// the composer's current ones, which is the difference between settling the parked
    /// copy and superseding it.
    async fn parked_save(
        &self,
        account: &AccountId,
        message_id: &MessageIdHeader,
    ) -> Option<(PendingOpId, u64)> {
        let queued = self.engine.outbox(account).await.ok()?;
        queued
            .iter()
            .filter(|row| row.kind == Some(PendingOpKind::MailDraftPut))
            .find_map(|row| {
                let put = serde_json::from_value::<DraftPut>(row.payload.clone()).ok()?;
                (put.draft.message_id == *message_id).then_some((row.id, digest(&put.draft)))
            })
    }

    /// Drops the composition and its hint, answering with what was known about it.
    ///
    /// The hint goes with it: a host that reuses a composition id must not open a fresh
    /// composer already saying the last one had saved.
    fn forget(&self, composition: &CompositionId) -> Option<Composition> {
        let record = {
            let mut drafts = self.drafts.lock().expect("drafts mutex poisoned");
            drafts.armed.remove(composition);
            drafts.open.remove(composition)
        };
        if self
            .draft_status
            .get()
            .is_some_and(|status| status.composition == composition.as_str())
        {
            self.draft_status.publish(None);
        }
        record
    }

    /// Publishes the in-flight hint.
    fn set_status_saving(&self, composition: &CompositionId) {
        self.publish_status(composition, DraftSaveState::Saving, String::new());
    }

    /// Publishes the landed hint, stamped with when the save landed.
    fn set_status_saved(&self, composition: &CompositionId) {
        self.publish_status(composition, DraftSaveState::Saved, now_instant());
    }

    /// Publishes the hint only when it moved: a save that ends where the last one did
    /// (the unchanged-content skip, the idle fire on an untouched composer) says nothing
    /// new and signals nothing.
    fn publish_status(&self, composition: &CompositionId, state: DraftSaveState, saved_at: String) {
        let snapshot = DraftStatusSnapshot {
            composition: composition.as_str().to_owned(),
            state,
            saved_at,
        };
        if self.draft_status.get().as_ref() != Some(&snapshot) {
            self.draft_status.publish(Some(snapshot));
        }
    }
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod tests;
