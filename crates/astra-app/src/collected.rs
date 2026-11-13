// SPDX-License-Identifier: GPL-3.0-only
//! The collected-key review surface (SP4d-2 D1): the app methods the desktop
//! review command drives. Split from the crate root (at the line cap) the way
//! `keys.rs` is: a child-module `impl App` block reading `self.crypto_store()`.
//!
//! Autocrypt/gossip consume stages peer keys silently into `collected_keys`;
//! this surface offers them to a person: list the staged rows, accept (promote
//! into `crypto_keys` plus a trust decision), accept as verified, or reject
//! (remove the staging row plus a rejected decision). The promote-plus-trust
//! pairing is load-bearing: `promote_collected_key` lands the key but writes no
//! decision, so a promoted-but-undecided key still answers Undecided in the
//! send resolver and the send gate aborts encryption. The explicit decision is
//! what makes the promoted key send-usable.
//!
//! **Privacy.** Collected keys are public material; no secret crosses this
//! surface and nothing here logs key material.

use astra_crypto_core::Standard;
use astra_crypto_store::{CollectedKeyRow, CryptoKeyRow};
use engine_api::{AccountId, Provider};

use crate::App;

/// Why a collected-key review action refused. The words name the refusal,
/// never the key material.
#[derive(Debug, thiserror::Error)]
pub enum CollectedReviewError {
    /// No crypto store is installed: nothing to review against.
    #[error("no crypto store is installed")]
    NoStore,
    /// The store refused the read or write.
    #[error("the key store refused the review action: {0}")]
    Store(String),
}

impl<P: Provider> App<P> {
    /// The account's staged keys across both standards, newest first (the
    /// store's `seen_at DESC, id DESC` ordering merged across the standards,
    /// so a same-second tie breaks on the row id). Public material only.
    ///
    /// # Errors
    /// Refuses when no store is installed or a read fails.
    pub fn list_collected(
        &self,
        account: &AccountId,
    ) -> Result<Vec<CollectedKeyRow>, CollectedReviewError> {
        let store = self.crypto_store().ok_or(CollectedReviewError::NoStore)?;
        let mut rows = astra_crypto_store::list_collected_keys_for_account(
            &store,
            account.as_str(),
            "openpgp",
        )
        .map_err(CollectedReviewError::Store)?;
        rows.extend(
            astra_crypto_store::list_collected_keys_for_account(&store, account.as_str(), "smime")
                .map_err(CollectedReviewError::Store)?,
        );
        rows.sort_by(|a, b| b.seen_at.cmp(&a.seen_at).then(b.id.cmp(&a.id)));
        Ok(rows)
    }

    /// Promotes a staged key into the keystore AND records the acceptance, so
    /// the promoted key is send-usable straight away. The store promote drains
    /// the staging rows and never downgrades a standing keystore row; the
    /// trust write is the load-bearing second half (the resolver answers
    /// Undecided without a decision). A `verified` choice always upgrades; an
    /// `unverified` choice records only when no decision stands, so a standing
    /// verified/personal decision is never downgraded. The acceptance binds
    /// `email`, or the staged row's peer email when none is named; a key with
    /// no peer email carries no address to decide against and still lands.
    ///
    /// # Errors
    /// Refuses when no store is installed, no staged or keystore row exists
    /// for the fingerprint, or a write fails.
    pub fn promote_collected(
        &self,
        account: &AccountId,
        standard: Standard,
        fingerprint: &str,
        email: Option<&str>,
        verified: bool,
    ) -> Result<CryptoKeyRow, CollectedReviewError> {
        let store = self.crypto_store().ok_or(CollectedReviewError::NoStore)?;
        // The address the acceptance binds: the caller's email, or the staged
        // row's peer email when none is named (the promote's own binding rule).
        // Read before the promote, which drains the staging rows.
        let effective_email = match email {
            Some(_) => email.map(str::to_owned),
            None => astra_crypto_store::list_collected_keys_for_fingerprint(
                &store,
                account.as_str(),
                standard.as_str(),
                fingerprint,
            )
            .map_err(CollectedReviewError::Store)?
            .into_iter()
            .next()
            .and_then(|row| row.peer_email),
        };
        let row = astra_crypto_store::promote_collected_key(
            &store,
            account.as_str(),
            standard.as_str(),
            fingerprint,
            email,
        )
        .map_err(CollectedReviewError::Store)?;
        astra_crypto_store::record_promote_trust(
            &store,
            account.as_str(),
            effective_email.as_deref(),
            standard.as_str(),
            fingerprint,
            verified,
        )
        .map_err(CollectedReviewError::Store)?;
        Ok(row)
    }

    /// Rejects a staged key: removes the staging row by id and records a
    /// `rejected` decision. Staging is decision-blind, so a re-opened source
    /// re-stages the key and the review list offers it again; the rejection
    /// stands (an unverified promote never downgrades it, and the resolver
    /// answers Rejected once a keystore row exists). The decision binds
    /// `email`, or the staged row's peer email when none is named (an empty
    /// address when neither stands, the unknown-key audit precedent).
    ///
    /// # Errors
    /// Refuses when no store is installed or a read or write fails.
    pub fn reject_collected(
        &self,
        account: &AccountId,
        id: i64,
        standard: Standard,
        fingerprint: &str,
        email: Option<&str>,
    ) -> Result<(), CollectedReviewError> {
        let store = self.crypto_store().ok_or(CollectedReviewError::NoStore)?;
        // The staged row's own peer email stands in when the caller names none,
        // so the rejected decision binds the address the key was staged for.
        let fallback = if email.is_none() {
            astra_crypto_store::list_collected_keys_for_fingerprint(
                &store,
                account.as_str(),
                standard.as_str(),
                fingerprint,
            )
            .map_err(CollectedReviewError::Store)?
            .into_iter()
            .find(|row| row.id == id)
            .and_then(|row| row.peer_email)
        } else {
            None
        };
        astra_crypto_store::remove_collected_key(&store, id)
            .map_err(CollectedReviewError::Store)?;
        let peer = email.map(str::to_owned).or(fallback).unwrap_or_default();
        astra_crypto_store::put_trust_decision(
            &store,
            account.as_str(),
            &peer,
            standard.as_str(),
            fingerprint,
            "rejected",
            None,
        )
        .map_err(CollectedReviewError::Store)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "collected_tests.rs"]
mod collected_tests;
