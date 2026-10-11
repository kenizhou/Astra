// SPDX-License-Identifier: GPL-3.0-only
//! The fake's contacts leg: the provider trait's minimal mail-free answer
//! and the contacts domain in full, each write recorded and applied to the
//! served copy the way a real CardDAV server's is, so the post-write
//! reconcile a facade write runs presents the change and the store the
//! snapshot reads holds what the user did.

use std::collections::BTreeMap;

use async_trait::async_trait;
use engine_api::{
    AccountId, AddressBookId, ContactDestination, ContactField, ContactFieldSet,
    ContactSourceClass, ContactsProvider, Provider, ProviderKey, WriteGuard,
};
use engine_core::contact::{ContactCard, ContactPatch, FieldPatch};
use engine_core::sync::{SyncObject, SyncState, SyncUpdate};
use engine_provider::{
    Capabilities, ConnectionInfo, ContactSourceSync, ContactWriteReceipt, ProviderError,
    ProviderResult, ScopeSync,
};

use super::PeopleFake;

#[async_trait]
impl Provider for PeopleFake {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none())
    }
}

impl engine_provider::MailboxWrites for PeopleFake {}
impl engine_provider::CalendarWrites for PeopleFake {}

#[async_trait]
impl ContactsProvider for PeopleFake {
    fn contact_destination(&self) -> Option<ContactDestination> {
        Some(ContactDestination {
            address_book: AddressBookId::try_from("book-1").unwrap(),
            source_class: ContactSourceClass::Personal,
            writable: self.writable,
            write_guard: Some(WriteGuard::Absent),
            supported_fields: ContactFieldSet::from_fields([
                ContactField::Kind,
                ContactField::Name,
                ContactField::Emails,
                ContactField::Phones,
                ContactField::Organizations,
                ContactField::Titles,
            ]),
        })
    }

    async fn sync_address_books(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ContactSourceSync<engine_api::AddressBook>> {
        let present = self
            .address_books
            .iter()
            .map(|book| book.id.key().clone())
            .collect();
        Ok(available(
            self.address_books.clone(),
            present,
            SyncState::new("ab-1"),
        ))
    }

    async fn sync_contacts(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ContactSourceSync<ContactCard>> {
        let cards = self.cards.lock().unwrap().clone();
        let present = cards.iter().map(|card| card.id.key().clone()).collect();
        Ok(available(cards, present, SyncState::new("cards-1")))
    }

    async fn fetch_contact(
        &self,
        _account: &AccountId,
        contact: &engine_api::ContactId,
    ) -> ProviderResult<ContactCard> {
        {
            let mut left = self.fail_fetches.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(ProviderError::retryable("the refetch was refused"));
            }
        }
        self.cards
            .lock()
            .unwrap()
            .iter()
            .find(|card| card.id == *contact)
            .cloned()
            .ok_or_else(|| ProviderError::invalid_state("the fake holds no such card"))
    }

    async fn create_contact(
        &self,
        _account: &AccountId,
        draft: &engine_api::ContactDraft,
    ) -> ProviderResult<ContactWriteReceipt> {
        if !self.consume_write_refusal() {
            return Err(ProviderError::invalid_state("the write was rejected"));
        }
        self.creates.lock().unwrap().push(draft.clone());
        let mut card = draft.card.clone();
        let served = self.cards.lock().unwrap().len();
        let minted = format!("c-{}", served + 1);
        card.id = engine_api::ContactId::try_from(minted.as_str()).unwrap();
        card.uid = Some(format!("uid-{}", served + 1));
        card.is_writable = true;
        self.cards.lock().unwrap().push(card.clone());
        Ok(ContactWriteReceipt::new(card.id))
    }

    async fn patch_contact(
        &self,
        _account: &AccountId,
        base: &ContactCard,
        patch: &ContactPatch,
    ) -> ProviderResult<ContactWriteReceipt> {
        if !self.consume_write_refusal() {
            return Err(ProviderError::invalid_state("the write was rejected"));
        }
        self.patches
            .lock()
            .unwrap()
            .push((base.clone(), patch.clone()));
        let mut cards = self.cards.lock().unwrap();
        if let Some(served) = cards.iter_mut().find(|served| served.id == base.id) {
            apply_patch(served, patch);
        }
        Ok(ContactWriteReceipt::new(base.id.clone()))
    }
}

/// Applies the patch's fields to the served copy, the subset this fake's
/// tests write. Decoded through the engine's own JSON because the typed
/// property maps decode the same way an adapter decodes them.
fn apply_patch(card: &mut ContactCard, patch: &ContactPatch) {
    for (field, edit) in &patch.fields {
        match (field, edit) {
            (ContactField::Name, FieldPatch::Set(value)) => {
                card.name = serde_json::from_value(value.clone()).ok();
            }
            (ContactField::Name, FieldPatch::Clear) => card.name = None,
            (ContactField::Emails, FieldPatch::Set(value)) => {
                card.emails = serde_json::from_value(value.clone()).unwrap_or_default();
            }
            (ContactField::Emails, FieldPatch::Clear) => card.emails = BTreeMap::default(),
            (ContactField::Phones, FieldPatch::Set(value)) => {
                card.phones = serde_json::from_value(value.clone()).unwrap_or_default();
            }
            (ContactField::Phones, FieldPatch::Clear) => card.phones = BTreeMap::default(),
            (ContactField::Organizations, FieldPatch::Set(value)) => {
                card.organizations = serde_json::from_value(value.clone()).unwrap_or_default();
            }
            (ContactField::Organizations, FieldPatch::Clear) => {
                card.organizations = BTreeMap::default();
            }
            (ContactField::Titles, FieldPatch::Set(value)) => {
                card.titles = serde_json::from_value(value.clone()).unwrap_or_default();
            }
            (ContactField::Titles, FieldPatch::Clear) => card.titles = BTreeMap::default(),
            _ => {}
        }
    }
}

/// One available source sync: a full snapshot of `items` with every key
/// named present.
fn available<T: SyncObject>(
    items: Vec<T>,
    present: std::collections::BTreeSet<ProviderKey>,
    state: SyncState,
) -> ContactSourceSync<T> {
    ContactSourceSync::Available {
        sync: ScopeSync::new(SyncUpdate::snapshot(items, present), state),
        cursor_recovered: false,
    }
}
