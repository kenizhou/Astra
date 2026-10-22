// SPDX-License-Identifier: GPL-3.0-only
//! The store-backed [`SendCryptoPolicy`]: every hook the core send gate
//! needs, bound to one account on the crypto store. kylins threaded a raw
//! pool through `apply_crypto`; the Astra gate takes the policy port, and
//! this is the implementation.

use astra_crypto_core::trust::RecipientTrustFailure;
use astra_crypto_core::{CryptoBackend, KeyHandleRef, SendCryptoPolicy, Standard};

use crate::store::CryptoStore;

/// An account-bound policy view of the crypto store for the send gate.
#[derive(Debug)]
pub struct StoreCryptoPolicy<'a> {
    store: &'a CryptoStore,
    account_id: String,
}

impl<'a> StoreCryptoPolicy<'a> {
    /// Bind the policy to `account_id` on `store`.
    pub fn new(store: &'a CryptoStore, account_id: impl Into<String>) -> Self {
        Self {
            store,
            account_id: account_id.into(),
        }
    }

    /// The bound store (the password arm and the outbox use it too).
    pub fn store(&self) -> &'a CryptoStore {
        self.store
    }

    /// The bound account id.
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
}

#[async_trait::async_trait]
impl SendCryptoPolicy for StoreCryptoPolicy<'_> {
    async fn recipient_trust_failures(
        &self,
        standard: Standard,
        emails: &[String],
    ) -> Vec<RecipientTrustFailure> {
        crate::recipient_trust_failures(self.store, &self.account_id, standard, emails)
    }

    async fn pin_preferred_handle(
        &self,
        standard: Standard,
        email: &str,
        handles: &[KeyHandleRef],
    ) -> Option<KeyHandleRef> {
        crate::pin_preferred_handle(self.store, standard, email, handles)
    }

    async fn trust_anchor_ders(&self) -> Result<Vec<Vec<u8>>, String> {
        crate::list_trust_anchor_certs(self.store, &self.account_id)
    }

    /// kylins' `validate_recipient_certs` ported: each recipient cert runs
    /// the same chain validator the receive side runs, under the S/MIME BR
    /// profile, with `from_email = None` (recipients are validated for
    /// chain + validity + leaf shape, NOT identity binding; their SAN
    /// doesn't need to match anything in the send context).
    /// `intermediates_der=&[]` because recipient certs in practice chain
    /// directly to a configured trust anchor.
    async fn validate_recipient_certs(
        &self,
        recipient_cert_ders: &[Vec<u8>],
        trust_anchor_ders: &[Vec<u8>],
        now_unix: u64,
    ) -> Result<(), String> {
        for (i, cert_der) in recipient_cert_ders.iter().enumerate() {
            let outcome = astra_crypto_smime::validate_signer_chain(
                cert_der,
                &[],
                trust_anchor_ders,
                None,
                i64::try_from(now_unix).unwrap_or(i64::MAX),
                &[],
            );
            if !outcome.chain_valid {
                return Err(format!(
                    "recipient cert #{} invalid: {}",
                    i,
                    outcome
                        .failure_reason
                        .as_deref()
                        .unwrap_or("(no failure reason)")
                ));
            }
        }
        Ok(())
    }

    async fn protect_subject(&self) -> Result<bool, String> {
        crate::crypto_policy::get_protect_subject(self.store, &self.account_id)
    }

    /// kylins' `maybe_prepend_gossip_headers` ported: prepend one
    /// `Autocrypt-Gossip:` header per non-sender recipient to the inner
    /// entity. Gating (TB `enigmailMsgComposeOverlay.js:1140-1145`): the
    /// account's `send_autocrypt_headers` setting is on AND there are >=2
    /// DISTINCT non-sender recipients. Per-recipient failures (export /
    /// parse / minimal export) skip that recipient: gossip is best-effort
    /// and must never block a send. `recipient_emails` and `recipients` are
    /// the parallel vectors the gate builds (sender first).
    async fn maybe_prepend_gossip_headers(
        &self,
        backend: &dyn CryptoBackend,
        entity: &[u8],
        account_email: &str,
        recipient_emails: &[String],
        recipients: &[KeyHandleRef],
    ) -> Vec<u8> {
        match crate::crypto_policy::get_send_autocrypt_headers(self.store, &self.account_id) {
            Ok(true) => {}
            Ok(false) => return entity.to_vec(),
            Err(e) => {
                log::warn!(
                    "[autocrypt] {} gossip setting read failed: {e}",
                    self.account_id
                );
                return entity.to_vec();
            }
        }
        // Distinct non-sender recipients (case-insensitive dedup), paired
        // with their resolved key handles.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut non_sender: Vec<(&str, &KeyHandleRef)> = Vec::new();
        for (email, handle) in recipient_emails.iter().zip(recipients.iter()) {
            if email.eq_ignore_ascii_case(account_email) {
                continue;
            }
            if seen.insert(email.to_ascii_lowercase()) {
                non_sender.push((email.as_str(), handle));
            }
        }
        if non_sender.len() < 2 {
            return entity.to_vec();
        }
        let mut headers: Vec<String> = Vec::with_capacity(non_sender.len());
        for (email, handle) in non_sender {
            let armored = match backend.export_public(&handle.handle).await {
                Ok(a) => a,
                Err(e) => {
                    log::warn!(
                        "[autocrypt] {} gossip export for {email} failed: {e}",
                        self.account_id
                    );
                    continue;
                }
            };
            let Some((_, cert)) = crate::autocrypt::parse_first_cert(&armored) else {
                log::warn!(
                    "[autocrypt] {} gossip: exported key for {email} has no cert",
                    self.account_id
                );
                continue;
            };
            match crate::autocrypt::minimal_export(&cert) {
                Ok(blob) => headers.push(crate::autocrypt::emit_gossip_header(email, &blob)),
                Err(e) => log::warn!(
                    "[autocrypt] {} gossip minimal export for {email} failed: {e}",
                    self.account_id
                ),
            }
        }
        if headers.is_empty() {
            return entity.to_vec();
        }
        crate::autocrypt::inject_headers(entity, &headers)
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;

#[cfg(test)]
#[path = "apply_store_tests.rs"]
mod apply_store_tests;
