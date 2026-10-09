// SPDX-License-Identifier: GPL-3.0-only
//! The per-composition record and its accessors: what the core knows about one
//! open composition, read and written under the drafts mutex, plus the parked
//! save a composition may still hold in the engine's outbox.
//!
//! A child of [`super`], split out so that file stays under the 500-line
//! limit; an `impl App` block, so every caller reads these as plain methods.

use engine_api::{
    AccountId, DraftPut, MessageIdHeader, PendingOpId, PendingOpKind, Provider, ProviderKey,
};

use super::{build::digest, Composition, CompositionId, HeldSave};
use crate::App;

impl<P: Provider> App<P> {
    /// Records `held` as the composition's current content, opening the composition's
    /// record when this is its first save, and answers the header every save of it
    /// carries. `None` only when a header could not be minted for a fresh record, which
    /// the caller treats as any other build failure.
    pub(super) fn record_held(
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
        let message_id = super::mint_message_id()?;
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
    pub(super) fn composition_account(&self, composition: &CompositionId) -> Option<AccountId> {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .map(|record| record.account.clone())
    }

    /// The header every save of this composition carries, or `None` when it is not
    /// open: the fire's proof that it is still the composition its arm was taken under.
    pub(super) fn composition_message_id(
        &self,
        composition: &CompositionId,
    ) -> Option<MessageIdHeader> {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .map(|record| record.message_id.clone())
    }

    /// What the server stores this composition under now.
    pub(super) fn composition_key(&self, composition: &CompositionId) -> Option<ProviderKey> {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .and_then(|record| record.key.clone())
    }

    /// Whether the last save put exactly this content on the server.
    pub(super) fn already_saved(&self, composition: &CompositionId, digest: u64) -> bool {
        self.drafts
            .lock()
            .expect("drafts mutex poisoned")
            .open
            .get(composition)
            .is_some_and(|record| record.saved == Some(digest))
    }

    /// Records what the save stored, and under which key.
    pub(super) fn record_save(&self, composition: &CompositionId, key: ProviderKey, digest: u64) {
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
    pub(super) async fn parked_save(
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
    pub(super) fn forget(&self, composition: &CompositionId) -> Option<Composition> {
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
}
