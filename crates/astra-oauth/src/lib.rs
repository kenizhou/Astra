// SPDX-License-Identifier: GPL-3.0-only
//! `astra-oauth`: the pre-registered half of Astra Mail's OAuth 2.0
//! Authorization-Code-with-PKCE client.
//!
//! The engine is OAuth-agnostic: it takes a bearer access token and nothing
//! more, so token acquisition and refresh live here in the product core. This
//! crate owns the pure, testable half of the flow; the platform halves (opening
//! the system browser, capturing the deep-link redirect, keeping tokens in the
//! OS keystore) live in the host:
//!
//! 1. [`Pkce::generate`] mints a verifier/challenge pair and [`random_state`]
//!    a CSRF `state`; [`OAuthProviderConfig::authorization_url`] builds the
//!    authorization URL the host opens.
//! 2. The host opens that URL in the system browser and captures the redirect
//!    back to the registered custom scheme.
//! 3. [`exchange_code`] trades the authorization `code` (plus the PKCE
//!    verifier) for a [`TokenSet`].
//! 4. [`refresh`] re-mints an access token from a stored refresh token.
//!
//! The token exchanges are pure functions over an injected HTTP client: the
//! caller passes the transport and so owns the TLS policy, and the unit tests
//! drive scripted responses.
//!
//! Nothing here is Microsoft-specific beyond
//! [`OAuthProviderConfig::microsoft`]; Google is just different endpoint and
//! scope values ([`OAuthProviderConfig::google`]). What a token endpoint's
//! refusal means for the stored grant is classified once, in
//! [`GrantRefusal`], never by matching on error text at a call site.

mod grant;
mod pkce;
mod provider;
pub mod scopes;
mod token;

pub use grant::{GrantRefusal, GrantedScopes};
pub use pkce::{random_state, Pkce};
pub use provider::{AuthStyle, OAuthProviderConfig, GOOGLE_SCOPES, MICROSOFT_GRAPH_SCOPES};
pub use token::{exchange_code, refresh, TokenSet};

/// A secret string (an access or refresh token) that redacts itself in
/// `Debug`, so a struct holding it can still derive or format `Debug`
/// without leaking the token.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wraps a raw secret.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The underlying secret. Call only at the point of use (building a
    /// request or persisting to the OS keystore), never to log or display it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Debug for Secret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// A failure somewhere in the OAuth flow.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    /// The HTTP request to the token endpoint failed (a network or TLS
    /// error); retryable.
    #[error("oauth transport: {0}")]
    Transport(#[source] reqwest::Error),
    /// The server answered, but its response could not be read: the
    /// connection died after the status line and before the body was
    /// complete.
    ///
    /// Deliberately not [`OAuthError::Transport`], even though both wrap the
    /// same `reqwest::Error`: a response that started arriving proves the
    /// request reached the server and its handler ran. On a refresh that
    /// means the refresh token presented is spent, and any rotated
    /// replacement was in the body just lost. Presenting the old token again
    /// is then a replay, and a server that ratchets refresh tokens treats a
    /// replay as theft and revokes the grant: the caller must not retry.
    #[error("oauth response lost after the server answered: {0}")]
    ResponseLost(#[source] reqwest::Error),
    /// The provider returned an OAuth error (RFC 6749 section 5.2).
    /// `invalid_grant` means the grant is expired or revoked and the person
    /// must sign in again; the other codes are usually configuration faults.
    #[error("oauth endpoint error: {error}; {}", .description.as_deref().unwrap_or("(no description)"))]
    Endpoint {
        /// The machine-readable error code (for example `invalid_grant`).
        error: String,
        /// The provider's human-readable detail, when present.
        description: Option<String>,
    },
    /// A success response could not be parsed as a token response: a
    /// protocol mismatch.
    #[error("decoding oauth response: {0}")]
    Decode(String),
}
