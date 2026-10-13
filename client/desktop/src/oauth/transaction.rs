// SPDX-License-Identifier: GPL-3.0-only
//! The OAuth credential transaction: the three answer paths the design's
//! D3 states, over the host's `AccountCredentialStore` port. The port
//! reports success and failure on both halves; what each answer means is
//! **when** the write was asked for, and this module is that "when",
//! named once:
//!
//! - **An add** whose persist fails rolls the account back and answers
//!   the error, so the setup surface the person is still looking at
//!   fails visibly. An account that works until the next launch and is
//!   then silently absent is the worse outcome, and an orphaned OAuth
//!   grant costs nothing. The rollback deletes best-effort: a platform
//!   store that errored after partially landing its write is exactly the
//!   half-there account the rule forbids, and a delete of an id that is
//!   not stored is a success by the port's own contract.
//! - **A rotation** whose persist fails cannot roll back, because the
//!   token it would revert to is already spent; the session continues
//!   from the token in memory and the failure is logged at error, saying
//!   what the next launch faces rather than what just did. The error is
//!   answered as well, so the caller that can surface it does.
//! - **A removal** whose delete fails answers the error without
//!   reverting: the account is gone from the runtime either way, and
//!   what survives an undeleted entry is a zombie account that
//!   reappears at the next launch with nothing to explain it.
//!
//! The transaction moves opaque config documents: the refresh token is
//! the single at-rest secret and rides inside the document, and the
//! never-persist-an-access-token rule binds whoever builds the document
//! (the provider slices), not this port, which cannot read one. The
//! tests prove the paths against a scripted store with a synthetic
//! grant round trip.

use std::sync::Arc;

use engine_api::AccountId;

use crate::credential_store::{AccountCredentialStore, CredentialStoreError};

/// The three answer paths over the host's credential store. One instance
/// wraps one store; the sign-in and refresh paths hold it beside the
/// flows that produce the credentials it moves.
pub(crate) struct CredentialTransaction {
    /// The OS secure store every write goes through.
    store: Arc<dyn AccountCredentialStore>,
}

impl CredentialTransaction {
    /// A transaction over `store`.
    #[must_use]
    pub(crate) fn new(store: Arc<dyn AccountCredentialStore>) -> Self {
        Self { store }
    }

    /// The add path: persists `config_toml` for `account`, rolling the
    /// add back and answering the error when the store refuses. First-
    /// time adds only: the rollback deletes whatever stood for the id,
    /// so a re-consent (which keeps the account) goes through
    /// [`rotate`](Self::rotate) instead.
    ///
    /// # Errors
    ///
    /// Returns the store's refusal; nothing stays stored for `account`.
    pub(crate) fn add(
        &self,
        account: &AccountId,
        config_toml: &str,
    ) -> Result<(), CredentialStoreError> {
        match self.store.persist(account.as_str(), config_toml) {
            Ok(()) => Ok(()),
            Err(err) => {
                // Roll the add back: the platform store refused, and a
                // store that errored after partially landing its write is
                // exactly the half-there account the rule forbids. The
                // delete is best-effort (a delete of an id that is not
                // stored is a success by the port's own contract); the
                // persist's own refusal stays the answer.
                if let Err(rollback) = self.store.delete(account.as_str()) {
                    log::warn!(
                        "oauth: the rollback after a refused persist could not run for [{}]: {rollback}",
                        account.as_str()
                    );
                }
                Err(err)
            }
        }
    }

    /// The rotation path: persists the rotated `config_toml` for
    /// `account`, logging at error what the next launch faces when the
    /// store refuses.
    ///
    /// # Errors
    ///
    /// Returns the store's refusal; the previous document stays stored.
    pub(crate) fn rotate(
        &self,
        account: &AccountId,
        config_toml: &str,
    ) -> Result<(), CredentialStoreError> {
        if let Err(err) = self.store.persist(account.as_str(), config_toml) {
            // Nothing can roll back: the token this rotation replaced is
            // already spent. The session continues from the token in
            // memory, and the next launch signs in with a dead grant, so
            // the line says what that launch faces rather than what just
            // happened. Not a warning: an hour later the account is gone.
            log::error!(
                "oauth: the rotated credential for [{}] could not be stored; the session continues \
                 from the token in memory, and the next launch faces the spent grant: {err}",
                account.as_str()
            );
            return Err(err);
        }
        Ok(())
    }

    /// The removal path: erases `account`'s stored document, answering
    /// the error without reverting when the store refuses.
    ///
    /// # Errors
    ///
    /// Returns the store's refusal; the entry stands as it was.
    pub(crate) fn remove(&self, account: &AccountId) -> Result<(), CredentialStoreError> {
        self.store.delete(account.as_str())
    }
}

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod tests;
