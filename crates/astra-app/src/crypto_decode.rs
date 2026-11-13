// SPDX-License-Identifier: GPL-3.0-only
//! The built-in crypto decoder: the reading pipeline's receive leg for both
//! standards (OpenPGP PGP/MIME and S/MIME enveloped). Where the plugin decoder
//! fails open (a plugin being down must not blank the user's mail), this
//! decoder fails CLOSED: a message the crypto layer recognises keeps its
//! verdict, and no key or a failed decrypt renders the banner alone, never the
//! ciphertext and never the plugin's answer.
//!
//! The unified orchestrator (`astra_crypto_store::receive`) sniffs the framing
//! once and routes to the matching per-standard orchestrator, which does the
//! detect, unwrap, decrypt, verify, and result-row work; this module maps its
//! neutral outcome onto the decode port: the decrypted legs (the seam re-runs
//! the identical sanitise pipeline on the HTML), the security banner, and the
//! restored protected subject. A source that is not the crypto framing answers
//! `None`, leaving the store's rendering and the plugin fold untouched.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content; the
//! legs flow only to the reading snapshot.

use std::sync::Arc;

use astra_crypto_core::{SignatureState, Standard};
use astra_crypto_store::{
    receive_with_fetcher, CrlFetcher, CryptoStore, NoopCrlFetcher, ReceiveOutcome, ReceiveState,
};
use astra_viewmodel::{AttachmentRow, SecurityBannerView, SecurityLevelView};

use crate::{DecodeOutcome, MessageDecoder, MessageRef};

/// The built-in OpenPGP receive decoder over the shared crypto store: the
/// account's private keys and collected certs resolve through it, and every
/// verdict lands its `message_crypto_results` row keyed by the message's
/// account and provider key. The S/MIME arm fetches-or-caches the signer
/// chain's CRLs through the attached [`CrlFetcher`] before the chain verify
/// (fail-soft); the default no-op fetcher keeps the offline path cache-only.
#[derive(Debug)]
pub struct CryptoDecoder {
    store: Arc<CryptoStore>,
    fetcher: Arc<dyn CrlFetcher>,
}

impl CryptoDecoder {
    /// A decoder reading its keys from `store`, with the offline no-op CRL
    /// fetcher (revocation uses only the already-cached CRLs).
    #[must_use]
    pub fn new(store: Arc<CryptoStore>) -> Self {
        Self::with_crl_fetcher(store, Arc::new(NoopCrlFetcher))
    }

    /// A decoder reading its keys from `store` and fetching CRLs through
    /// `fetcher` (the host's bounded HTTP implementation).
    #[must_use]
    pub fn with_crl_fetcher(store: Arc<CryptoStore>, fetcher: Arc<dyn CrlFetcher>) -> Self {
        Self { store, fetcher }
    }
}

#[async_trait::async_trait]
impl MessageDecoder for CryptoDecoder {
    async fn decode(&self, message: &MessageRef, raw: &[u8]) -> Option<DecodeOutcome> {
        let outcome = receive_with_fetcher(
            &self.store,
            message.account.as_str(),
            message.key.as_str(),
            raw,
            self.fetcher.as_ref(),
        )
        .await;
        match outcome.state {
            // Not the crypto framing: the store's rendering and the plugin
            // fold stand untouched.
            ReceiveState::NotEncrypted => None,
            // Fail closed: the banner alone, never the ciphertext, never a
            // fall-through to the plugin.
            ReceiveState::NoKey => Some(closed("Encrypted message; cannot decrypt (no key).")),
            ReceiveState::Failed => Some(closed("Encrypted message; cannot decrypt.")),
            ReceiveState::Decrypted => Some(decrypted(&outcome)),
        }
    }
}

/// A fail-closed answer: the error banner and nothing else. No legs, no
/// subject; the verdict flag tells the seam the store's own outer-framing
/// read must not render and the ciphertext chips must hide.
fn closed(text: &str) -> DecodeOutcome {
    DecodeOutcome {
        banner: Some(SecurityBannerView {
            level: SecurityLevelView::Error,
            text: text.to_owned(),
        }),
        crypto_verdict: true,
        ..Default::default()
    }
}

/// The decrypted answer: the legs (the seam re-runs the identical sanitise
/// pipeline on the HTML), the restored protected subject, and the banner
/// stating the signature verdict. An unknown signer degrades to a warning,
/// never a failure: the words still render.
fn decrypted(outcome: &ReceiveOutcome) -> DecodeOutcome {
    let (level, text) = banner_words(outcome);
    DecodeOutcome {
        html: outcome.html.clone(),
        plain: outcome.plain.clone(),
        banner: Some(SecurityBannerView {
            level,
            text: text.to_owned(),
        }),
        subject: outcome.subject.clone(),
        crypto_verdict: true,
    }
}

/// The banner level and words for a decrypted outcome. The standard
/// discriminator drives the wording: S/MIME names the cert-chain verdicts,
/// OpenPGP keeps the key-trust vocabulary.
fn banner_words(outcome: &ReceiveOutcome) -> (SecurityLevelView, &'static str) {
    if matches!(outcome.standard, Standard::Smime) {
        return smime_words(outcome);
    }
    openpgp_words(outcome.signature_state)
}

/// The S/MIME banner arms: the cert-chain verdicts (verified / unverified /
/// mismatch / invalid). A revoked chain hard-failure is distinguished by the
/// revocation state. Every non-verified arm is a warning, never a failure: the
/// words still render.
fn smime_words(outcome: &ReceiveOutcome) -> (SecurityLevelView, &'static str) {
    match outcome.signature_state {
        SignatureState::ValidVerified => (
            SecurityLevelView::Info,
            "Encrypted message; signature verified.",
        ),
        SignatureState::ValidUnverified | SignatureState::UnknownKey => (
            SecurityLevelView::Warning,
            "Encrypted message; signature unverified.",
        ),
        SignatureState::Mismatch => (
            SecurityLevelView::Warning,
            "Encrypted message; From address does not match the signer certificate.",
        ),
        SignatureState::Invalid => {
            if outcome.revocation_state.as_deref() == Some("revoked") {
                (
                    SecurityLevelView::Warning,
                    "Encrypted message; signature invalid (certificate revoked).",
                )
            } else {
                (
                    SecurityLevelView::Warning,
                    "Encrypted message; signature invalid.",
                )
            }
        }
        SignatureState::NotSigned => (SecurityLevelView::Info, "Encrypted message; not signed."),
    }
}

/// The OpenPGP banner arms (the key-trust vocabulary). An unknown signer
/// degrades to a warning; the remaining verdicts (unverified, invalid,
/// mismatch) are the honest warning: decrypted, but the signature did not
/// verify.
fn openpgp_words(state: SignatureState) -> (SecurityLevelView, &'static str) {
    match state {
        SignatureState::ValidVerified => (
            SecurityLevelView::Info,
            "Encrypted message; signature verified.",
        ),
        SignatureState::UnknownKey => (
            SecurityLevelView::Warning,
            "Encrypted message; signature unverified (unknown signer).",
        ),
        SignatureState::NotSigned => (SecurityLevelView::Info, "Encrypted message; not signed."),
        _ => (
            SecurityLevelView::Warning,
            "Encrypted message; signature unverified.",
        ),
    }
}

/// Drops the ciphertext pseudo-attachment chips from a recognised crypto
/// message's attachment list: the `encrypted.asc` / `smime.p7m` part is the
/// framing's transport, not a file the reader downloads. The
/// `hide_invitation_parts` precedent, keyed on the crypto verdict instead of
/// the invitation card.
pub(crate) fn hide_crypto_parts(attachments: &mut Vec<AttachmentRow>) {
    attachments.retain(|row| !is_crypto_pseudo_part(row));
}

/// Whether an attachment row is the crypto framing's transport part: the
/// conventional ciphertext file names, or a crypto control media type. An
/// ordinary `application/octet-stream` file keeps its row.
fn is_crypto_pseudo_part(row: &AttachmentRow) -> bool {
    let name = row.file_name.to_ascii_lowercase();
    name == "encrypted.asc"
        || name == "smime.p7m"
        || name == "smime.p7s"
        || row
            .media_type
            .eq_ignore_ascii_case("application/pgp-encrypted")
        || row
            .media_type
            .eq_ignore_ascii_case("application/pkcs7-mime")
        || row
            .media_type
            .eq_ignore_ascii_case("application/x-pkcs7-mime")
}

#[cfg(test)]
#[path = "crypto_decode_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "crypto_decode_smime_tests.rs"]
mod smime_tests;
