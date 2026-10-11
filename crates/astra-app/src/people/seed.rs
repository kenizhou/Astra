// SPDX-License-Identifier: GPL-3.0-only
//! The editor's seed: the stored card a reference names, read through the
//! engine and rendered as the editor's own input subset. The editor opens on
//! what the card carries, never on the merged detail (D4), so a form save
//! patches the card it seeded from and no other account's copy moves.

use engine_api::{ContactCard, Provider};

use crate::people::cards::{base_first_flat_value, base_name_parts};
use crate::protocol::{ContactDraftInput, ContactNameInput};
use crate::reference::CardRef;
use crate::App;

impl<P: Provider> App<P> {
    /// The editor's seed for one card: the stored card `card` names, in the
    /// editor's own field shape. `None` when the store holds no such card,
    /// which is also what a card whose every copy has since been tombstoned
    /// reads as: nothing to open the form on. A store failure is logged and
    /// answered the same way, because a form that cannot seed must not open
    /// half-seeded.
    pub async fn card_seed(&self, card: &CardRef) -> Option<ContactDraftInput> {
        let stored = match self.engine.contact_card(&card.account, &card.card).await {
            Ok(stored) => stored,
            Err(err) => {
                log::warn!("people: the card read failed: {err}");
                return None;
            }
        };
        stored.map(|stored| seed_of(&stored))
    }
}

/// Renders one stored card as the editor's input subset: the name's two
/// fields (whole into the given field when the card carries no structured
/// name, per the seeding rule), the first organisation and title, and both
/// lists in card order.
fn seed_of(card: &ContactCard) -> ContactDraftInput {
    let (given, surname) = base_name_parts(card);
    ContactDraftInput {
        name: card
            .name
            .is_some()
            .then_some(ContactNameInput { given, surname }),
        organisation: base_first_flat_value(card, "organizations", "name"),
        role: base_first_flat_value(card, "titles", "name"),
        emails: card
            .emails
            .values()
            .map(|property| property.value.address.clone())
            .collect(),
        phones: card
            .phones
            .values()
            .map(|property| property.value.number.clone())
            .collect(),
    }
}

#[cfg(test)]
#[path = "../people_seed_tests.rs"]
mod tests;
