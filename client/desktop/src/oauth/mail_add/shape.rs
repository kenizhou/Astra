// SPDX-License-Identifier: GPL-3.0-only
//! The mail sign-in's shape-keeping halves, split from the completion
//! for the file cap: what a re-consent carries over from the standing
//! document, and the rollback that restores a standing account exactly
//! as it stood when a dial or store refuses.

use std::sync::Arc;

use astra_account::{AccountConfig, ImapConnections};
use engine_api::AccountId;

use super::AccountDials;

/// Carries what an existing account stored over to the document a new
/// sign-in for it produced: the pinned id and every section the grant
/// does not own, because a re-consent changes the grant and nothing
/// else. The sections the fresh document does not write (a stored
/// submission host, a settings-written crypto method) survive the
/// re-consent whole, and a certificate accepted on this very walk joins
/// the stored list rather than replacing it: the account must trust the
/// certificate it just accepted at the next launch too.
pub(super) fn keep_stored_shape(signed_in: &mut AccountConfig, stored: Option<AccountConfig>) {
    let Some(stored) = stored else {
        return;
    };
    signed_in.shape.id = stored.shape.id;
    if signed_in.dav.is_none() {
        signed_in.dav = stored.dav;
    }
    if signed_in.smtp.is_none() {
        signed_in.smtp = stored.smtp;
    }
    if signed_in.crypto.is_none() {
        signed_in.crypto = stored.crypto;
    }
    let mut exceptions = stored.certificate_exceptions;
    for accepted in &signed_in.certificate_exceptions {
        if !exceptions.contains(accepted) {
            exceptions.push(accepted.clone());
        }
    }
    signed_in.certificate_exceptions = exceptions;
}

/// Takes the pre-registration back: a new add's entry goes whole, a
/// re-consent's entry is put back as it stood, config and pool
/// together, because a failed dial or a refused store must leave a live
/// account dialling exactly as it did.
pub(super) fn roll_back(
    dials: &AccountDials,
    id: &AccountId,
    stored: Option<AccountConfig>,
    stored_pool: Option<Arc<ImapConnections>>,
) {
    match (stored, stored_pool) {
        (Some(config), Some(pool)) => {
            if let Err(err) = dials.register(id, config, pool) {
                log::warn!("oauth: a rolled-back account's entry could not be restored: {err}");
            }
        }
        _ => {
            if let Err(err) = dials.remove(id.as_str()) {
                log::warn!(
                    "oauth: a rolled-back account's id stayed in the dial table's index: {err}"
                );
            }
        }
    }
}
