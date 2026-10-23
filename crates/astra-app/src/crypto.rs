// SPDX-License-Identifier: GPL-3.0-only
//! The built-in crypto send path (SP4b spec D3): the Astra-side bypass for
//! accounts whose crypto method is set. The pinned engine's submit takes
//! only `Draft` payloads, so a crypto send assembles here with the engine's
//! own `engine_rfc5322` (byte-identical output), wraps with the ported
//! `apply_crypto` gate, and queues the wrapped bytes in the Astra-side
//! crypto outbox, drained by the small explicit bypass transport.
//!
//! The host installs the support ([`App::with_crypto_send`]): the shared
//! crypto store and the resolver that answers one account's crypto method
//! and dial table (endpoints, credentials, and the trust-policy connector,
//! none of which the app can see). With neither installed every submit
//! rides the engine's queue untouched, the zero-cost default.
//!
//! The outbox half (the drain, the snapshot rows, and the verb routing)
//! lives in [`crate::crypto_outbox`], the same split `submit` and `outbox`
//! take over the engine's queue.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content,
//! its recipients, or its subject; counts and ids only.

use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use astra_crypto_core::{
    apply_crypto, AddressSpec, CryptoMethod, CryptoPolicy, CryptoSendError, SendDraft, Standard,
};
use astra_crypto_store::{enqueue_crypto_send, CryptoStore, SqliteKeyStore, StoreCryptoPolicy};
use engine_api::{AccountId, Draft, EmailAddress, Provider};
use tokio_rustls::TlsConnector;

use crate::App;

/// The composer's per-draft crypto toggles on a submit intent. `None`
/// means "the account's policy default": with a crypto method configured
/// the default is to sign and encrypt (the account opted into crypto);
/// the flags are only consulted when a method is set, so a plain account's
/// send is untouched either way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptoFlags {
    /// Sign the message; defaults to `true` under a crypto method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign: Option<bool>,
    /// Encrypt the message; defaults to `true` under a crypto method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypt: Option<bool>,
}

impl CryptoFlags {
    /// The resolved toggles under a configured crypto method.
    pub(crate) fn resolved(self) -> (bool, bool) {
        (self.sign.unwrap_or(true), self.encrypt.unwrap_or(true))
    }
}

/// One account's crypto dial table, answered by the host's resolver: the
/// endpoints and credentials the bypass transport dials, and the
/// trust-policy connector it dials them through. Secrets live in-process
/// only; none of this crosses an IPC boundary.
#[derive(Clone)]
pub struct CryptoTransport {
    /// The account's crypto method (`openpgp` / `smime`).
    pub method: CryptoMethod,
    /// The account's own address (the encrypt-to-self recipient and the
    /// envelope's `MAIL FROM`).
    pub account_email: String,
    /// The SMTP submission endpoint.
    pub smtp: astra_crypto_store::SmtpDial,
    /// The IMAP endpoint the Sent copy files through.
    pub imap: astra_crypto_store::ImapDial,
    /// The trust-policy connector both endpoints dial through (the host
    /// builds it, certificate exceptions included).
    pub connector: TlsConnector,
}

impl core::fmt::Debug for CryptoTransport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The dial table carries secrets; which halves stand is the whole
        // safe description.
        f.debug_struct("CryptoTransport")
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

/// The host's answer to "how does this account send crypto mail": its
/// crypto method (the gate's switch) and, when one is set, its dial table.
/// A method without a dial table is the typed unavailable answer the spec
/// names: the send holds with the cause, never a silent drop.
#[async_trait]
pub trait CryptoTransportResolver: Send + Sync {
    /// The account's configured crypto method (`None` when unset: the
    /// engine's queue keeps the send).
    async fn crypto_method(&self, account: &AccountId) -> CryptoMethod;

    /// The account's crypto dial table, or `None` when it has no IMAP/SMTP
    /// submission settings stored (a Graph/Google/JMAP account, or an IMAP
    /// account without SMTP).
    async fn crypto_transport(&self, account: &AccountId) -> Option<CryptoTransport>;
}

/// The installed crypto send support: the shared store and the resolver.
#[derive(Clone)]
pub(crate) struct CryptoSendSupport {
    /// The crypto store the gate reads and the outbox queues in.
    pub(crate) store: Arc<CryptoStore>,
    /// The host's per-account crypto answers.
    pub(crate) resolver: Arc<dyn CryptoTransportResolver>,
}

impl<P: Provider> App<P> {
    /// Installs the built-in crypto send path: the crypto store every
    /// gated send reads and queues in, and the host's resolver for each
    /// account's method and dial table. Call before the first dispatch.
    #[must_use]
    pub fn with_crypto_send(
        mut self,
        store: Arc<CryptoStore>,
        resolver: Arc<dyn CryptoTransportResolver>,
    ) -> Self {
        self.crypto_send = std::sync::Mutex::new(Some(CryptoSendSupport { store, resolver }));
        self
    }

    /// Installs the crypto send support after boot (the host's assembly
    /// installs the settings seam the same way), so the support lands
    /// without a boot-signature change.
    pub fn install_crypto_send(
        &self,
        store: Arc<CryptoStore>,
        resolver: Arc<dyn CryptoTransportResolver>,
    ) {
        *self.crypto_send.lock().expect("crypto-send mutex poisoned") =
            Some(CryptoSendSupport { store, resolver });
    }

    /// The installed support, cloned out from behind its mutex.
    pub(crate) fn crypto_support(&self) -> Option<CryptoSendSupport> {
        self.crypto_send
            .lock()
            .expect("crypto-send mutex poisoned")
            .clone()
    }

    /// The crypto branch of the submit tail (`submit_and_drain`): when the
    /// account's crypto method is set, assemble with the engine's own
    /// assembler, gate with `apply_crypto`, queue the wrapped bytes in the
    /// crypto outbox, and drain, answering `true` (the engine's queue is
    /// not involved). Any other account answers `false` and the engine
    /// path keeps the send, the zero-cost default.
    pub(crate) async fn crypto_submit_maybe(
        &self,
        from: &AccountId,
        draft: &Draft,
        sign: bool,
        encrypt: bool,
    ) -> bool {
        let Some(support) = self.crypto_support() else {
            return false;
        };
        let method = support.resolver.crypto_method(from).await;
        let Some(standard) = method.standard() else {
            return false;
        };
        let at = self.account_ordinal(from).await;
        let Some(transport) = support.resolver.crypto_transport(from).await else {
            log::warn!(
                "send[a{at}]: crypto is configured but the account has no IMAP/SMTP submission settings"
            );
            self.hold_send(
                from.clone(),
                draft.clone(),
                CryptoFlags {
                    sign: Some(sign),
                    encrypt: Some(encrypt),
                },
                "crypto".to_string(),
                "crypto is configured for this account, but it has no IMAP/SMTP submission settings"
                    .to_string(),
                astra_viewmodel::HeldState::Blocked,
            )
            .await;
            return true;
        };
        // The gate's reads and the outbox's rows key on the account id; the
        // store's minimal account row (the FK target and the put() email
        // binding) is registered idempotently here.
        if let Err(err) = support
            .store
            .register_account(from.as_str(), &transport.account_email)
        {
            log::warn!("send[a{at}]: the crypto store refused the account registration: {err}");
            return true;
        }

        // Assemble with the engine's own assembler: byte-identical to a
        // non-crypto send, filed variant (with the `Bcc` header) wrapped
        // once; the wire variant is the same ciphertext minus the header.
        let date = time::OffsetDateTime::now_utc();
        let filed_mime = match engine_rfc5322::assemble_filed_message(draft, date) {
            Ok(bytes) => bytes,
            Err(err) => {
                log::warn!("send[a{at}]: the message could not be assembled: {err}");
                return true;
            }
        };
        let send_draft = SendDraft {
            to: draft.to.iter().map(address_spec).collect(),
            cc: draft.cc.iter().map(address_spec).collect(),
            bcc: draft.bcc.iter().map(address_spec).collect(),
            sign,
            encrypt,
            standard_override: None,
        };
        let signer = match astra_crypto_store::get_default_signing_key(
            &support.store,
            from.as_str(),
            standard,
        ) {
            Ok(row) => row.map(|row| row.as_core()),
            Err(err) => {
                log::warn!("send[a{at}]: the default signing key lookup failed: {err}");
                None
            }
        };
        let outcome = match standard {
            Standard::Smime => {
                let backend = astra_crypto_smime::SmimeBackend::new(
                    Arc::new(SqliteKeyStore::new(support.store.clone(), from.as_str())),
                    CryptoPolicy::default(),
                );
                apply_crypto(
                    &backend,
                    standard,
                    &SqliteKeyStore::new(support.store.clone(), from.as_str()),
                    &filed_mime,
                    &send_draft,
                    &transport.account_email,
                    signer.as_ref(),
                    &StoreCryptoPolicy::new(&support.store, from.as_str()),
                )
                .await
            }
            Standard::OpenPgp => {
                let backend = astra_crypto_openpgp::OpenpgpBackend::new(
                    Arc::new(SqliteKeyStore::new(support.store.clone(), from.as_str())),
                    CryptoPolicy::default(),
                );
                apply_crypto(
                    &backend,
                    standard,
                    &SqliteKeyStore::new(support.store.clone(), from.as_str()),
                    &filed_mime,
                    &send_draft,
                    &transport.account_email,
                    signer.as_ref(),
                    &StoreCryptoPolicy::new(&support.store, from.as_str()),
                )
                .await
            }
            Standard::Sm => Err(CryptoSendError::Backend(
                astra_crypto_core::CryptoError::UnsupportedStandard("sm not implemented".into()),
            )),
        };
        let wrapped = match outcome {
            Ok(wrapped) => wrapped,
            Err(failure) => {
                log::info!("send[a{at}]: the crypto gate held the send: {failure}");
                self.hold_send(
                    from.clone(),
                    draft.clone(),
                    CryptoFlags {
                        sign: Some(sign),
                        encrypt: Some(encrypt),
                    },
                    "crypto".to_string(),
                    gate_failure_words(&failure),
                    astra_viewmodel::HeldState::Blocked,
                )
                .await;
                return true;
            }
        };
        let wire_bytes = strip_bcc_header(&wrapped);
        let recipients: Vec<String> = draft
            .to
            .iter()
            .chain(&draft.cc)
            .chain(&draft.bcc)
            .map(|a| a.email.clone())
            .collect();
        let row_id = match enqueue_crypto_send(
            &support.store,
            from.as_str(),
            standard.as_str(),
            &draft.subject,
            &wire_bytes,
            &wrapped,
            &transport.account_email,
            &recipients,
        ) {
            Ok(id) => id,
            Err(err) => {
                log::warn!("send[a{at}]: the crypto outbox refused the send: {err}");
                return true;
            }
        };
        log::info!("send[a{at}]: queued a crypto send (row {row_id})");
        self.drain_crypto_outbox().await;
        self.rebuild_sending().await;
        self.invalidate_list_cache();
        true
    }
}

/// The reason a held crypto row shows, in product words.
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
#[path = "crypto_tests.rs"]
mod crypto_tests;
