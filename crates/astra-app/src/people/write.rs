// SPDX-License-Identifier: GPL-3.0-only
//! The people write paths: the create and edit intents dispatched onto the
//! engine's contact verbs, under the discipline the write contract states.
//!
//! - **An edit names a card, and the patch is built on what that card
//!   actually carries.** The base is re-read from the store
//!   ([`Engine::contact_card`]), never the merged detail a surface was
//!   showing, so a rebuild of one field keeps every property the form cannot
//!   state. A field the edit does not name is left out of the patch
//!   entirely, and an edit that changed nothing sends nothing.
//! - **A create must be findable.** A draft with no name, no organisation,
//!   and no address names nothing to file the card under, and the core
//!   refuses it before anything is sent: the `Invalid` write state, the app's
//!   own refusal (the engine's create validation does not carry this rule).
//! - **Anything but a clean reconcile landed.** A write the engine accepted
//!   but could not re-read is `FailedWithRetry` (unconfirmed, not rejected);
//!   its recovery is the re-read the next refresh performs, never the same
//!   write issued again.
//!
//! The shapes themselves live in [`super::cards`], the mapping onto the
//! engine's draft and patch over the JSON seam.
//!
//! [`Engine::contact_card`]: engine_api::Engine::contact_card

use std::sync::Arc;

use astra_viewmodel::PeopleWriteState;
use engine_api::{AccountId, ContactReconciled, ContactsProvider, Provider};

use crate::people::cards::{build_create_draft, build_patch, is_findable};
use crate::people::ContactsLeg;
use crate::protocol::{ContactDraftInput, ContactPatchInput};
use crate::reference::{AddressBookRef, CardRef};
use crate::App;

impl<P: Provider> App<P> {
    /// The create arm: refused unless findable, routed through the leg whose
    /// destination is the named book, marked in flight before the engine
    /// call, and settled from the engine's own reconcile.
    pub(crate) async fn create_person_contact(
        &self,
        book: &AddressBookRef,
        draft: &ContactDraftInput,
    ) {
        let leg = self.people_legs(&book.account).into_iter().find(|leg| {
            leg.contact_destination().is_some_and(|destination| {
                destination.writable && destination.address_book == book.book
            })
        });
        let Some(leg) = leg else {
            log::info!("people: the address book has no writable leg; the create was refused");
            self.refuse_people_write(&book.account).await;
            return;
        };
        if !is_findable(draft) {
            log::info!("people: the create names nothing to file it under; it was refused");
            self.refuse_people_write(&book.account).await;
            return;
        }
        let engine_draft = match build_create_draft(book, draft) {
            Ok(engine_draft) => engine_draft,
            Err(err) => {
                log::warn!("people: the create could not be built: {err}");
                return;
            }
        };
        self.mark_people_write(&book.account, PeopleWriteState::Writing);
        self.rebuild_people().await;
        match self
            .engine
            .create_contact(
                leg.as_ref(),
                &book.account,
                &people_idempotency(),
                &engine_draft,
            )
            .await
        {
            Ok(write) => {
                let at = self.account_ordinal(&book.account).await;
                log::info!("people[a{at}]: a contact was created");
                self.settle_people_write(&book.account, &write.reconciled)
                    .await;
            }
            Err(err) => {
                log::warn!("people: the create was refused: {err}");
                self.fail_people_write(&book.account).await;
            }
        }
    }

    /// The edit arm: the base card re-read from the store, the patch rebuilt
    /// on top of it field by field, nothing sent when nothing changed.
    pub(crate) async fn update_person_contact(&self, card: &CardRef, patch: &ContactPatchInput) {
        let Some(leg) = self.people_writable_leg(&card.account) else {
            log::info!("people: the account has no writable contacts leg; the edit was refused");
            return;
        };
        let base = match self.engine.contact_card(&card.account, &card.card).await {
            Ok(Some(base)) => base,
            Ok(None) => {
                log::info!("people: the card is no longer held; the edit was refused");
                return;
            }
            Err(err) => {
                log::warn!("people: the card read failed: {err}");
                return;
            }
        };
        let engine_patch = build_patch(&base, patch);
        if engine_patch.kind.is_none() && engine_patch.fields.is_empty() {
            log::info!("people: the edit changed nothing; nothing was sent");
            self.rebuild_people().await;
            return;
        }
        self.mark_people_write(&card.account, PeopleWriteState::Writing);
        self.rebuild_people().await;
        match self
            .engine
            .patch_contact(
                leg.as_ref(),
                &card.account,
                &people_idempotency(),
                &base,
                &engine_patch,
            )
            .await
        {
            Ok(write) => {
                let at = self.account_ordinal(&card.account).await;
                log::info!("people[a{at}]: a contact was edited");
                self.settle_people_write(&card.account, &write.reconciled)
                    .await;
            }
            Err(err) => {
                log::warn!("people: the edit was refused: {err}");
                self.fail_people_write(&card.account).await;
            }
        }
    }

    /// `account`'s first writable contacts leg, if one is installed.
    fn people_writable_leg(&self, account: &AccountId) -> Option<Arc<ContactsLeg>> {
        self.people_legs(account).into_iter().find(|leg| {
            leg.contact_destination()
                .is_some_and(|destination| destination.writable)
        })
    }

    /// Records the app's own refusal of a form: the `Invalid` state, whose
    /// sentence a surface states under the form the user is still looking at.
    async fn refuse_people_write(&self, account: &AccountId) {
        self.mark_people_write(account, PeopleWriteState::Invalid);
        self.rebuild_people().await;
    }

    /// Records a write state for `account`, replacing whatever stood there:
    /// the next write resolves the last one's row.
    pub(crate) fn mark_people_write(&self, account: &AccountId, state: PeopleWriteState) {
        self.people_state
            .lock()
            .expect("people mutex poisoned")
            .writes
            .insert(account.clone(), state);
    }

    /// Clears `account`'s write state: the recovery a successful re-read or a
    /// cleanly settled write performs.
    pub(crate) fn clear_people_write(&self, account: &AccountId) {
        self.people_state
            .lock()
            .expect("people mutex poisoned")
            .writes
            .remove(account);
    }

    /// Settles a landed write's outcome: `Applied` clears the account's row,
    /// anything else marks it `FailedWithRetry` (the write landed; only the
    /// local confirmation is missing), and either way the rebuilt snapshot is
    /// what tells the surface.
    pub(crate) async fn settle_people_write(
        &self,
        account: &AccountId,
        reconciled: &ContactReconciled,
    ) {
        match reconciled {
            ContactReconciled::Applied(_) => self.clear_people_write(account),
            // Busy and Failed both mean the write landed and only the local
            // copy is stale, so the recovery for either is the re-read a
            // refresh performs.
            _ => self.mark_people_write(account, PeopleWriteState::FailedWithRetry),
        }
        self.rebuild_people().await;
    }

    /// Marks the account's row for a write the engine refused outright: the
    /// change went nowhere, and a surface that watched the write start must
    /// also see that it did not settle.
    async fn fail_people_write(&self, account: &AccountId) {
        self.mark_people_write(account, PeopleWriteState::FailedWithRetry);
        self.rebuild_people().await;
    }
}

/// A unique-per-intent idempotency key for a people write, from the wall
/// clock and a process counter, the same mint the calendar writes use.
fn people_idempotency() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("people:{nanos}:{seq}")
}

#[cfg(test)]
#[path = "../people_write_tests.rs"]
mod tests;
