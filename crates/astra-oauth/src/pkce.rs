// SPDX-License-Identifier: GPL-3.0-only
//! PKCE (RFC 7636): the proof key a public client (a desktop app that can
//! keep no client secret) sends so a stolen authorization `code` is useless
//! without the matching verifier.
//!
//! A high-entropy code verifier is minted from the system CSPRNG (32 random
//! bytes, 43 base64url characters) and its code challenge derived as
//! `base64url(SHA-256(verifier))`. The challenge travels in the front-channel
//! authorization request; the verifier is sent only on the back-channel token
//! exchange. `S256` is the only method used (never the `plain` fallback). A
//! separate [`random_state`] value covers CSRF on the redirect.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};

/// A PKCE verifier/challenge pair for one authorization request.
///
/// The verifier is secret (it stands in for a client secret), so it is
/// redacted in `Debug`; the challenge travels in the front-channel
/// authorization URL and is not sensitive.
#[derive(Clone)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    /// Generates a fresh pair from the system CSPRNG: a 43-character
    /// base64url verifier (32 random bytes, within RFC 7636's 43..=128
    /// range) and its `S256` challenge.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        SystemRandom::new()
            .fill(&mut bytes)
            .expect("system CSPRNG fills 32 bytes");
        Self::from_verifier(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Rebuilds the pair from a stored `verifier`, recomputing the `S256`
    /// challenge, so a host that persisted only the verifier between the
    /// begin and complete steps of a flow can reconstruct it.
    #[must_use]
    pub fn from_verifier(verifier: String) -> Self {
        let digest = digest::digest(&digest::SHA256, verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest.as_ref());
        Self {
            verifier,
            challenge,
        }
    }

    /// The secret code verifier, sent only on the back-channel token
    /// exchange.
    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// The `S256` code challenge, placed in the authorization request URL.
    #[must_use]
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

impl core::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never leak the verifier; the challenge is not sensitive but is
        // noise in logs.
        f.debug_struct("Pkce")
            .field("verifier", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// A fresh, unguessable `state` value (16 random bytes as base64url) binding
/// one authorization request to its redirect, so a callback that does not
/// echo it back is rejected as forged (CSRF).
#[must_use]
pub fn random_state() -> String {
    let mut bytes = [0u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("system CSPRNG fills 16 bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
#[path = "pkce_tests.rs"]
mod tests;
