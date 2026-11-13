// SPDX-License-Identifier: GPL-3.0-only
//! The reading pane's password re-entry (SP4c-3c D2): the second leg of the
//! W9 receive flow. The first open answers needs-password (a SKESK message no
//! key fits); the reader supplies a passphrase through the desktop command,
//! and this module re-runs the password orchestrator over a fresh
//! `engine.message_source` and republishes the reading snapshot with the
//! decrypted legs (or the re-prompt signal on a wrong passphrase).
//!
//! Split from `reading.rs` (near the line cap) so the open pipeline and the
//! password re-entry each stay under it. The re-entry reuses the identical
//! render seam: the password outcome is mapped through the shared decoder
//! mapping and injected as a one-shot decoder, so the decrypted legs take the
//! same sanitise / inline-cid / remote-gate pipeline, the restored subject and
//! the hidden ciphertext chips land exactly as a key-path open, and the
//! prompt affordance rides the snapshot.
//!
//! **Privacy.** The passphrase is held only in memory: it moves into the
//! orchestrator as a [`SecretBox`] (zeroized on drop) and is never logged or
//! persisted. A cancel never reaches here; the verdict stays needs-password.

use std::sync::Arc;

use astra_crypto_core::SecretBox;
use astra_crypto_store::CryptoStore;
use astra_viewmodel::PasswordPromptState;
use engine_api::Provider;

use crate::{
    crypto_decode::decode_outcome,
    reading::{render_reading, resolve},
    App, DecodeOutcome, MessageDecoder, MessageRef,
};

impl<P: Provider> App<P> {
    /// Installs the shared crypto store the password re-entry decrypts
    /// through: the same instance the built-in decoder reads its keys from.
    /// Call it before the first passphrase submission; an app with no store
    /// installed stays needs-password (fail closed, nothing to decrypt
    /// against).
    pub fn install_crypto_store(&self, store: Arc<CryptoStore>) {
        *self
            .crypto_store
            .lock()
            .expect("crypto-store mutex poisoned") = Some(store);
    }

    /// The installed crypto store, cloned out from behind its mutex.
    pub(crate) fn crypto_store(&self) -> Option<Arc<CryptoStore>> {
        self.crypto_store
            .lock()
            .expect("crypto-store mutex poisoned")
            .clone()
    }

    /// Re-opens the passphrase-protected `message` with the reader's
    /// `passphrase`: fetch a fresh source, run the password orchestrator, and
    /// republish the reading snapshot through the identical render seam.
    /// Answers the prompt affordance the snapshot now carries: `None` once
    /// the passphrase decrypts (the words render, the dialog closes) or on a
    /// structural failure (the closed banner stands), `Some(WrongPassword)`
    /// when the passphrase fit no slot (the dialog re-prompts).
    ///
    /// The publish is guarded by the same still-open check the open and the
    /// refresh use: a close or a newer open that lands while the decrypt is in
    /// flight drops this render. A message the store no longer holds, or an
    /// account with no provider, publishes nothing.
    ///
    /// **Privacy.** The passphrase moves into the orchestrator as a
    /// [`SecretBox`] and is never logged or persisted.
    pub async fn submit_reading_password(
        &self,
        message: MessageRef,
        passphrase: SecretBox<String>,
    ) -> Option<PasswordPromptState> {
        // No store installed: nothing to decrypt against, stay needs-password.
        let Some(store) = self.crypto_store() else {
            return Some(PasswordPromptState::NeedsPassword);
        };
        let handle = self.account_handle(&message.account).await?;
        let provider = handle.providers.first()?;
        let original = resolve(&self.engine, &message).await?;
        // A fresh source read (cache-first): the orchestrator decrypts the
        // bytes the engine holds now, not a stale copy.
        let Ok(source) = self
            .engine
            .message_source(provider, &message.account, &original)
            .await
        else {
            return Some(PasswordPromptState::NeedsPassword);
        };
        let outcome = astra_crypto_store::receive_openpgp_password(
            &store,
            message.account.as_str(),
            message.key.as_str(),
            source.as_bytes(),
            passphrase,
        )
        .await;
        // The shared mapping lands the identical verdict shape the open uses;
        // its affordance is exactly what the snapshot will carry.
        let decoded = decode_outcome(&outcome)?;
        let prompt = decoded.password_prompt;
        // Re-render through the identical seam with the password outcome
        // injected as a one-shot decoder: the decrypted legs take the same
        // sanitise pipeline, the subject restores, the ciphertext chips hide.
        let one_shot: Arc<dyn MessageDecoder> = Arc::new(PasswordOutcomeDecoder(decoded));
        if let Some((snapshot, _)) =
            render_reading(&self.engine, provider, &message, Some(&one_shot)).await
        {
            if self.still_open(&message) {
                self.reading.publish(Some(snapshot));
            }
        }
        prompt
    }
}

/// A one-shot decoder carrying the password orchestrator's mapped outcome:
/// the re-entry's render seam consults it instead of the installed composite
/// (whose key path would answer needs-password again). It ignores the raw
/// source the seam fetches and answers the pre-computed verdict, so the
/// decrypted legs flow through the identical pipeline a key-path open uses.
struct PasswordOutcomeDecoder(DecodeOutcome);

#[async_trait::async_trait]
impl MessageDecoder for PasswordOutcomeDecoder {
    async fn decode(&self, _message: &MessageRef, _raw: &[u8]) -> Option<DecodeOutcome> {
        Some(self.0.clone())
    }
}

#[cfg(test)]
#[path = "reading_password_tests.rs"]
mod tests;
