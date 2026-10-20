// SPDX-License-Identifier: GPL-3.0-only
//! The signature rules a compose open applies: the per-account assignment the
//! preferences persist, the block an open seeds into the document, and the
//! swap a From change makes of a seeded signature the person has not touched.
//!
//! The rules live here as core-shaped helpers over the composer's own
//! [`Block`] and [`Signature`] types, so every surface that opens or re-picks
//! a composition drives one definition of them. The block itself rides the
//! document as retained content the trimmed editor shows verbatim: the
//! signature is authored once in Settings and seeded, never re-typed.

use astra_composer::{Block, Signature};
use engine_api::AccountId;
use serde::{Deserialize, Serialize};

use crate::{Preferences, PreferencesState};

/// One account's signature assignment: what its new mail opens with and what
/// its replies open with. The same signature may serve both, and either slot
/// may stand empty while the other holds one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSignatures {
    /// What a plain new message from this account seeds; `None` seeds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_mail: Option<Signature>,
    /// What a reply or forward from this account seeds; `None` seeds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<Signature>,
}

impl Preferences {
    /// The signature `account` seeds a composition with: the reply slot's own
    /// when `reply`, else the new-mail slot's. `None` for an account with no
    /// assignment or an empty slot, which seeds nothing.
    #[must_use]
    pub fn seeded_signature(&self, account: &AccountId, reply: bool) -> Option<Signature> {
        let assigned = self.signatures.get(account)?;
        if reply {
            assigned.reply.clone()
        } else {
            assigned.new_mail.clone()
        }
    }

    /// Records one slot of `account`'s assignment. `None` clears the slot, and
    /// the account's last empty slot takes its row with it, so the persisted
    /// file holds only live assignments.
    pub fn set_seeded_signature(
        &mut self,
        account: &AccountId,
        reply: bool,
        signature: Option<Signature>,
    ) {
        if signature.is_none() && !self.signatures.contains_key(account) {
            return;
        }
        let assigned = self.signatures.entry(account.clone()).or_default();
        if reply {
            assigned.reply = signature;
        } else {
            assigned.new_mail = signature;
        }
        if self
            .signatures
            .get(account)
            .is_some_and(|row| row.new_mail.is_none() && row.reply.is_none())
        {
            self.signatures.remove(account);
        }
    }

    /// The account plain compositions open from: the persisted preference
    /// when it names one of `accounts`, else the first of them. A preference
    /// naming an account that no longer stands answers the first account
    /// rather than one no mail can leave from; `None` only when no account
    /// stands at all.
    #[must_use]
    pub fn send_account(&self, accounts: &[AccountId]) -> Option<AccountId> {
        let first = accounts.first()?;
        let held = self
            .default_send_account
            .as_ref()
            .filter(|preferred| accounts.contains(*preferred));
        Some(held.unwrap_or(first).clone())
    }

    /// Records the default send account; `None` unsets it, which the first
    /// standing account then answers.
    pub fn set_send_account(&mut self, account: Option<AccountId>) {
        self.default_send_account = account;
    }
}

impl PreferencesState {
    /// The account plain compositions open from, the persisted preference
    /// resolved against `accounts` exactly as
    /// [`Preferences::send_account`](crate::Preferences::send_account) does.
    pub(crate) fn send_account(&self, accounts: &[AccountId]) -> Option<AccountId> {
        self.prefs.send_account(accounts)
    }
}

/// Seeds `signature` into a composition's blocks: directly beneath the quoted
/// original when the document carries one (a reply types above its sign-off,
/// the quote stands above both), otherwise at the top (a new mail, where the
/// person's words grow under the signature). A document already bearing a
/// signature seeds nothing: a resumed draft or a re-opened composition owns
/// the one it has.
///
/// The blocks are the core-built document, before a surface reorders its
/// quotes: the position reads the core's order, and a surface that then
/// lifts the quote above the typed words keeps the signature below both.
#[must_use]
pub fn seed_signature(blocks: Vec<Block>, signature: &Signature) -> Vec<Block> {
    if blocks
        .iter()
        .any(|block| matches!(block, Block::Signature(_)))
    {
        return blocks;
    }
    let at = blocks
        .iter()
        .rposition(|block| matches!(block, Block::Quote(_)))
        .map_or(0, |index| index + 1);
    let mut blocks = blocks;
    blocks.insert(at, Block::Signature(signature.clone()));
    blocks
}

/// Replaces every block that still is `seeded` with `incoming`, or drops it
/// when the new account seeds nothing: the untouched-swap rule a From change
/// drives. A signature the person edited no longer equals the seeded one and
/// stays untouched, because an edited signature is the person's.
#[must_use]
pub fn swap_seeded(
    blocks: Vec<Block>,
    seeded: &Signature,
    incoming: Option<&Signature>,
) -> Vec<Block> {
    blocks
        .into_iter()
        .filter_map(|block| match block {
            Block::Signature(ref current) if current == seeded => {
                incoming.map(|next| Block::Signature(next.clone()))
            }
            other => Some(other),
        })
        .collect()
}

#[cfg(test)]
#[path = "signatures_tests.rs"]
mod tests;
