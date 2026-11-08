// SPDX-License-Identifier: GPL-3.0-only
//! The store-bound [`MimeWrapper`]: the engine's crypto seam (ECS spec)
//! implemented over this crate. The engine assembles the filed variant (with
//! the `Bcc` header) and hands it here once; this wraps it with the ported
//! `apply_crypto` gate and answers both variants: the wire one the same
//! ciphertext minus the `Bcc` header, exactly the semantics SP4b's bypass
//! transport shipped, now inside the one engine outbox.
//!
//! A refusal crosses as [`ProviderError::permanent`] carrying the gate's
//! reason in product words, so the engine settles the op `Failed` before any
//! socket dials and the user retries with the engine's own Send Now once the
//! keys are fixed. Construction is fail-closed: an account whose store
//! registration fails gets a wrapper that always refuses, never one that
//! falls back to plaintext.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content,
//! its recipients, or its subject; counts and ids only.

use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use engine_core::mail::EmailAddress;
use engine_provider::{Draft, MimeWrapOutcome, MimeWrapper, ProviderError};

use astra_crypto_core::{
    apply_crypto, AddressSpec, CryptoMethod, CryptoPolicy, CryptoSendError, SendDraft, Standard,
};

use crate::{CryptoStore, SqliteKeyStore, StoreCryptoPolicy};

/// The account's installed wrapper: wraps each asked-for send through the
/// ported gate, reading keys and trust from the shared crypto store.
pub struct StoreMimeWrapper {
    store: Arc<CryptoStore>,
    account_id: String,
    account_email: String,
    standard: Standard,
}

impl StoreMimeWrapper {
    /// The wrapper for one account under `standard`, bound to the shared
    /// store. Construction cannot fail: anything that can go wrong (the
    /// account row, the signing-key lookup, the gate itself) surfaces at
    /// `wrap` time as a refusal.
    #[must_use]
    pub fn new(
        store: Arc<CryptoStore>,
        account_id: &str,
        account_email: &str,
        standard: Standard,
    ) -> Self {
        Self {
            store,
            account_id: account_id.to_owned(),
            account_email: account_email.to_owned(),
            standard,
        }
    }
}

impl core::fmt::Debug for StoreMimeWrapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The store and the account id are not secrets, but the safe
        // description is which standard the account wraps under.
        f.debug_struct("StoreMimeWrapper")
            .field("standard", &self.standard)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MimeWrapper for StoreMimeWrapper {
    async fn wrap(
        &self,
        draft: &Draft,
        filed_message: Vec<u8>,
    ) -> Result<MimeWrapOutcome, ProviderError> {
        // The gate's reads key on the account id; the store's minimal account
        // row (the FK target and the put() email binding) is registered
        // idempotently here. A store that cannot take the row fails closed.
        self.store
            .register_account(&self.account_id, &self.account_email)
            .map_err(|err| {
                ProviderError::permanent(format!(
                    "the crypto store refused the account registration: {err}"
                ))
            })?;
        let send_draft = SendDraft {
            to: draft.to.iter().map(address_spec).collect(),
            cc: draft.cc.iter().map(address_spec).collect(),
            bcc: draft.bcc.iter().map(address_spec).collect(),
            sign: draft.sign,
            encrypt: draft.encrypt,
            standard_override: None,
        };
        let signer =
            match crate::get_default_signing_key(&self.store, &self.account_id, self.standard) {
                Ok(row) => row.map(|row| row.as_core()),
                Err(err) => {
                    return Err(ProviderError::permanent(format!(
                        "the default signing key lookup failed: {err}"
                    )));
                }
            };
        let keystore = || SqliteKeyStore::new(self.store.clone(), &self.account_id);
        let outcome = match self.standard {
            Standard::Smime => {
                let backend = astra_crypto_smime::SmimeBackend::new(
                    Arc::new(keystore()),
                    CryptoPolicy::default(),
                );
                apply_crypto(
                    &backend,
                    self.standard,
                    &keystore(),
                    &filed_message,
                    &send_draft,
                    &self.account_email,
                    signer.as_ref(),
                    &StoreCryptoPolicy::new(&self.store, &self.account_id),
                )
                .await
            }
            Standard::OpenPgp => {
                let backend = astra_crypto_openpgp::OpenpgpBackend::new(
                    Arc::new(keystore()),
                    CryptoPolicy::default(),
                );
                apply_crypto(
                    &backend,
                    self.standard,
                    &keystore(),
                    &filed_message,
                    &send_draft,
                    &self.account_email,
                    signer.as_ref(),
                    &StoreCryptoPolicy::new(&self.store, &self.account_id),
                )
                .await
            }
            Standard::Sm => Err(CryptoSendError::Backend(
                astra_crypto_core::CryptoError::UnsupportedStandard("sm not implemented".into()),
            )),
        };
        let wrapped =
            outcome.map_err(|failure| ProviderError::permanent(gate_failure_words(&failure)))?;
        Ok(MimeWrapOutcome {
            wire: strip_bcc_header(&wrapped),
            filed: wrapped,
        })
    }
}

/// A wrapper that refuses every send with `reason`: the fail-closed answer
/// for an account whose wrapper could not be built (a crypto method is set,
/// so sending plaintext is never the right fallback).
struct AlwaysRefusing(String);

#[async_trait]
impl MimeWrapper for AlwaysRefusing {
    async fn wrap(
        &self,
        _draft: &Draft,
        _filed_message: Vec<u8>,
    ) -> Result<MimeWrapOutcome, ProviderError> {
        Err(ProviderError::permanent(self.0.clone()))
    }
}

/// The wrapper one account's dial installs, or `None` when the account has
/// no crypto method set (the zero-cost default: sends go out plainly, flags
/// inert). Fail-closed: a store failure yields a wrapper that refuses every
/// crypto send with the cause, never a silent plaintext path.
#[must_use]
pub fn mime_wrapper_for(
    store: &Arc<CryptoStore>,
    account_id: &str,
    account_email: &str,
    method: CryptoMethod,
) -> Option<Arc<dyn MimeWrapper>> {
    let standard = method.standard()?;
    if let Err(err) = store.register_account(account_id, account_email) {
        return Some(Arc::new(AlwaysRefusing(format!(
            "the crypto store refused the account registration: {err}"
        ))));
    }
    Some(Arc::new(StoreMimeWrapper::new(
        store.clone(),
        account_id,
        account_email,
        standard,
    )))
}

/// The reason a refused send shows, in product words.
fn gate_failure_words(failure: &CryptoSendError) -> String {
    match failure {
        CryptoSendError::RecipientTrust(failures) => {
            let mut words = String::from("not every recipient can be encrypted to: ");
            for (i, f) in failures.iter().enumerate() {
                if i > 0 {
                    words.push_str(", ");
                }
                let _ = write!(words, "{} ({})", f.email, f.state.as_str());
            }
            words
        }
        other => other.to_string(),
    }
}

/// One address for the gate's draft slice.
fn address_spec(address: &EmailAddress) -> AddressSpec {
    AddressSpec {
        name: address.name.clone(),
        email: address.email.clone(),
    }
}

/// Strip the `Bcc` header (and any folded continuation lines) from a wrapped
/// message's outer header block: the filed variant carries it, the wire
/// variant never does, and both share the one ciphertext. Header-insensitive
/// below the first blank line: a body line starting `Bcc:` is content, not
/// a header.
fn strip_bcc_header(wrapped: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(wrapped) else {
        return wrapped.to_vec();
    };
    let Some(blank) = text.find("\r\n\r\n") else {
        return wrapped.to_vec();
    };
    let (headers, rest) = text.split_at(blank);
    let mut kept: Vec<&str> = Vec::new();
    let mut in_bcc = false;
    for line in headers.split("\r\n") {
        let is_continuation = line.starts_with(' ') || line.starts_with('\t');
        if is_continuation {
            if !in_bcc {
                kept.push(line);
            }
            continue;
        }
        let name = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        in_bcc = name == "bcc";
        if !in_bcc {
            kept.push(line);
        }
    }
    let mut out = kept.join("\r\n").into_bytes();
    out.extend_from_slice(rest.as_bytes());
    out
}

#[cfg(test)]
#[path = "wrap_tests.rs"]
mod wrap_tests;
