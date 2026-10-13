// SPDX-License-Identifier: GPL-3.0-only
//! `astra-oauth`: Astra Mail's OAuth 2.0 Authorization-Code-with-PKCE
//! client.
//!
//! The engine is OAuth-agnostic: it takes a bearer access token and
//! nothing more, so token acquisition and refresh live here in the
//! product core. This crate owns the pure, testable half of the flow; the
//! platform halves (opening the system browser, capturing the deep-link
//! redirect, keeping tokens in the OS keystore) live in the host:
//!
//! 1. [`OAuthClient::begin`] mints a [`Pkce`] pair + CSRF `state` and
//!    builds the authorization URL the host opens.
//! 2. The host opens that URL in the system browser and captures the
//!    redirect back to the registered custom scheme.
//! 3. [`OAuthClient::complete`] validates the returned `state`, then
//!    exchanges the authorization `code` (+ PKCE verifier) for a
//!    [`TokenSet`].
//! 4. [`OAuthClient::refresh`] re-mints an access token from a stored
//!    refresh token.
//!
//! For a provider Astra has **no** pre-registered client with (a
//! self-hosted JMAP server, or any server never met before):
//! [`discover_protected_resource`], [`discover_auth_server`] and
//! [`register_client`] find the endpoints and mint a client id from the
//! standards (RFC 9728, then RFC 8414, then RFC 7591) before step 1
//! above.
//!
//! The pre-registered clients a build does carry (Google's and
//! Microsoft's) are injected at compile time and live in
//! [`credentials`], the only place in the tree that holds one. A build
//! given none still connects mail: the routes that need a registration
//! are simply not offered.
//!
//! The token exchanges are pure functions over an injected HTTP client:
//! the caller passes the transport and so owns the TLS policy, and the
//! unit tests drive scripted responses.
//!
//! Nothing here is Microsoft-specific beyond
//! [`OAuthProviderConfig::microsoft`]; Google is just different endpoint
//! and scope values ([`OAuthProviderConfig::google`]). What a token
//! endpoint's refusal means for the stored grant is classified once, in
//! [`GrantRefusal`], never by matching on error text at a call site.

pub mod credentials;
mod discovery;
mod grant;
mod mail_providers;
mod pkce;
mod provider;
mod reach;
mod register;
pub mod scopes;
mod token;

use std::collections::HashMap;

pub use discovery::{
    discover_auth_server, discover_protected_resource, discovery_client, discovery_client_with,
    AuthServerMetadata, DiscoveryError, ProtectedResource,
};
pub use grant::{GrantRefusal, GrantedScopes};
pub use mail_providers::{provider_for_host, static_mail_providers, StaticMailProvider};
pub use pkce::{random_state, Pkce};
pub use provider::{AuthStyle, OAuthProviderConfig, GOOGLE_SCOPES, MICROSOFT_GRAPH_SCOPES};
pub use reach::TokenRequestReach;
pub use register::{grants_mail_access, register_client, select_scopes, RegisteredClient};
use time::OffsetDateTime;
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
    /// request or persisting to the OS keystore), never to log or display
    /// it.
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
    /// Building the shared TLS policy for OAuth HTTPS requests failed.
    #[error("oauth tls: {0}")]
    Tls(#[source] engine_tls::TlsError),
    /// The HTTP request to the token endpoint failed (a network or TLS
    /// error); retryable.
    #[error("oauth transport: {0}")]
    Transport(#[source] reqwest::Error),
    /// The server answered, but its response could not be read: the
    /// connection died after the status line and before the body was
    /// complete.
    ///
    /// Deliberately not [`OAuthError::Transport`], even though both wrap
    /// the same `reqwest::Error`: a response that started arriving proves
    /// the request reached the server and its handler ran. On a refresh
    /// that means the refresh token presented is spent, and any rotated
    /// replacement was in the body just lost. Presenting the old token
    /// again is then a replay, and a server that ratchets refresh tokens
    /// treats a replay as theft and revokes the grant: the caller must
    /// not retry. See [`reach`](Self::reach) for the classification.
    #[error("oauth response lost after the server answered: {0}")]
    ResponseLost(#[source] reqwest::Error),
    /// The provider returned an OAuth error (RFC 6749 section 5.2).
    /// `invalid_grant` means the grant is expired or revoked and the
    /// person must sign in again; the other codes are usually
    /// configuration faults.
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
    /// The redirect callback was malformed or forged: unparseable,
    /// missing `code`, or a `state` that doesn't match the one issued (a
    /// possible CSRF).
    #[error("oauth callback: {0}")]
    Callback(String),
}

/// The output of [`OAuthClient::begin`]: the URL to open, and the
/// `state` + [`Pkce`] the caller must hold until the redirect comes back
/// (to validate `state` and to send the verifier on the exchange).
#[derive(Debug)]
pub struct AuthRequest {
    /// The authorization URL to open in the system browser.
    pub authorization_url: String,
    /// The CSRF `state` issued for this request; the callback must echo
    /// it.
    pub state: String,
    /// The PKCE pair; its verifier is sent on [`OAuthClient::complete`].
    pub pkce: Pkce,
}

/// An OAuth client bound to one [`OAuthProviderConfig`], wrapping a
/// reused HTTP client.
#[derive(Debug)]
pub struct OAuthClient {
    http: reqwest::Client,
    provider: OAuthProviderConfig,
}

impl OAuthClient {
    /// The scopes this client asks for on an authorization request.
    ///
    /// What a grant ends up carrying can be less (a service issues what
    /// it can) so this is the "requested" half
    /// [`GrantedScopes::from_response`] needs to read a response that
    /// named none.
    #[must_use]
    pub fn requested_scopes(&self) -> &[String] {
        &self.provider.scopes
    }

    /// Builds a client for `provider`.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Tls`] if the shared TLS policy cannot be
    /// built, or [`OAuthError::Transport`] if the underlying HTTP client
    /// cannot be built.
    pub fn new(provider: OAuthProviderConfig) -> Result<Self, OAuthError> {
        let tls = engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system())
            .map_err(OAuthError::Tls)?;
        let http = tls
            .reqwest_builder()
            .build()
            .map_err(OAuthError::Transport)?;
        Ok(Self { http, provider })
    }

    /// This client's provider config.
    #[must_use]
    pub fn provider(&self) -> &OAuthProviderConfig {
        &self.provider
    }

    /// Starts a flow: mints a fresh PKCE pair + `state` and builds the
    /// authorization URL. The caller opens the URL and holds the returned
    /// `state`/verifier for [`OAuthClient::complete`]. `login_hint` (the
    /// address being connected, when known) pre-fills and targets that
    /// account instead of showing the picker.
    #[must_use]
    pub fn begin(&self, login_hint: Option<&str>) -> AuthRequest {
        let pkce = Pkce::generate();
        let state = random_state();
        let authorization_url =
            self.provider
                .authorization_url(&state, pkce.challenge(), login_hint);
        AuthRequest {
            authorization_url,
            state,
            pkce,
        }
    }

    /// Completes a flow from the raw redirect `callback_url`: validates
    /// the echoed `state`, and the `iss` when the provider config expects
    /// one, then exchanges the `code` (+ PKCE verifier) for tokens. `now`
    /// timestamps the access token's absolute expiry.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Callback`] if the callback is malformed, its
    /// `state` doesn't match, or its `iss` names a different
    /// authorization server; [`OAuthError::Endpoint`] if the provider
    /// carried an `error` back or rejects the exchange; or
    /// [`OAuthError::Transport`]/[`OAuthError::Decode`] on a
    /// transport/parse failure.
    pub async fn complete(
        &self,
        callback_url: &str,
        expected_state: &str,
        pkce_verifier: &str,
        now: OffsetDateTime,
    ) -> Result<TokenSet, OAuthError> {
        let code = parse_callback(
            expected_state,
            self.provider.expected_issuer.as_deref(),
            callback_url,
        )?;
        exchange_code(&self.http, &self.provider, &code, pkce_verifier, now).await
    }

    /// Redeems a stored `refresh_token` for a fresh [`TokenSet`].
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Endpoint`] if the refresh token is
    /// revoked/expired (`invalid_grant`; re-authenticate), or a
    /// transport/decode error otherwise.
    pub async fn refresh(
        &self,
        refresh_token: &str,
        now: OffsetDateTime,
    ) -> Result<TokenSet, OAuthError> {
        refresh(&self.http, &self.provider, refresh_token, now).await
    }
}

/// Parses a redirect `callback_url`, returning the authorization `code`
/// once the echoed `state` matches `expected_state` and, when
/// `expected_issuer` is given, the `iss` names that same authorization
/// server.
///
/// Handles the custom-scheme redirect the OS auth session hands back
/// (for example `com.kenizhou.astra://oauth?code=...&state=...`). A
/// provider `error` in the callback, a missing `code`, a missing or
/// mismatched `state`, or an unparseable URL are all rejected.
///
/// `expected_issuer` is `Some` only for a server that **advertised** it
/// sends `iss` (RFC 9207); the parameter is then mandatory and a response
/// without one is refused, because a stripped `iss` is precisely what a
/// mix-up attack produces. `None` means the server never claimed to send
/// one, so there is nothing to check.
///
/// # Errors
///
/// Returns [`OAuthError::Endpoint`] if the callback carries a provider
/// `error`, or [`OAuthError::Callback`] if it is unparseable, forged (a
/// `state` or `iss` mismatch), or missing the `code`.
pub fn parse_callback(
    expected_state: &str,
    expected_issuer: Option<&str>,
    callback_url: &str,
) -> Result<String, OAuthError> {
    let url = url::Url::parse(callback_url)
        .map_err(|err| OAuthError::Callback(format!("unparseable redirect URL: {err}")))?;
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    if let Some(error) = params.get("error") {
        return Err(OAuthError::Endpoint {
            error: error.clone(),
            description: params.get("error_description").cloned(),
        });
    }
    match params.get("state") {
        Some(state) if state == expected_state => {}
        Some(_) => {
            return Err(OAuthError::Callback(
                "state mismatch (possible CSRF)".to_owned(),
            ));
        }
        None => return Err(OAuthError::Callback("callback missing state".to_owned())),
    }
    if let Some(expected) = expected_issuer {
        check_issuer(expected, params.get("iss").map(String::as_str))?;
    }
    params
        .get("code")
        .cloned()
        .ok_or_else(|| OAuthError::Callback("callback missing code".to_owned()))
}

/// Checks an authorization response's `iss` against the issuer whose
/// metadata we read (RFC 9207 section 2.4). A trailing slash is not
/// significant in an issuer identifier.
///
/// An absent `iss` is a failure rather than a pass. The check only runs
/// for a server that said it sends one, and the attack it defends against
/// is a relay that hands us another server's code: treating "it did not
/// arrive" as "nothing to verify" would let the attacker turn the check
/// off by deleting a query parameter.
fn check_issuer(expected: &str, received: Option<&str>) -> Result<(), OAuthError> {
    let Some(received) = received else {
        return Err(OAuthError::Callback(
            "callback missing iss, which this server advertises it sends".to_owned(),
        ));
    };
    if received.trim_end_matches('/') == expected.trim_end_matches('/') {
        return Ok(());
    }
    Err(OAuthError::Callback(format!(
        "iss names {received}, not the authorization server this flow began with"
    )))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;
