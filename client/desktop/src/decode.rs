// SPDX-License-Identifier: GPL-3.0-only
//! The composite message decoder: the built-in crypto verdict alongside the
//! plugin decode, the reading pipeline's one installed decoder.
//!
//! The crypto decoder is consulted first. A message it recognises keeps its
//! verdict whatever the answer: decrypted legs, or the fail-closed banner a
//! no-key or failed decrypt carries. The plugin decoder never sees a
//! recognised message, so its fail-open cannot blank or replace a crypto
//! verdict. Everything the crypto layer does not recognise falls through to
//! the plugin, whose own fail-open stands: a plugin being down must not blank
//! the user's mail.

use std::sync::Arc;

use astra_app::{DecodeOutcome, MessageDecoder, MessageRef};

/// The reading decode fold: crypto verdict first, plugin fall-through second.
pub(crate) struct CompositeDecoder {
    crypto: Arc<dyn MessageDecoder>,
    plugin: Arc<dyn MessageDecoder>,
}

impl CompositeDecoder {
    /// A fold consulting `crypto` before `plugin`.
    #[must_use]
    pub(crate) fn new(crypto: Arc<dyn MessageDecoder>, plugin: Arc<dyn MessageDecoder>) -> Self {
        Self { crypto, plugin }
    }
}

#[async_trait::async_trait]
impl MessageDecoder for CompositeDecoder {
    async fn decode(&self, message: &MessageRef, raw: &[u8]) -> Option<DecodeOutcome> {
        // A crypto verdict (any Some answer) takes precedence and ends the
        // fold: a recognised message never falls through to the plugin.
        if let Some(outcome) = self.crypto.decode(message, raw).await {
            return Some(outcome);
        }
        self.plugin.decode(message, raw).await
    }
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
