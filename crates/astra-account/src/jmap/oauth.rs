// SPDX-License-Identifier: GPL-3.0-only
//! "Sign in with your provider" for a JMAP account: the standards
//! discovery chain (RFC 9728 to find the authorization server from the
//! session's own `401`, RFC 8414 to read its endpoints, RFC 7591 to
//! register this install as a client), the grant a completion persists,
//! and the re-authentication ([`reauth`]) that re-runs from that grant.
//!
//! Microsoft and Google are *integrated*: their endpoints, and this app's
//! client id, are known at build time. A JMAP server is not. It may be
//! Fastmail, or a Stalwart someone runs on a NAS, and we have no
//! registration with either, so the whole flow is discovered from the
//! server's own metadata before the first authorization URL is built.
//!
//! # Discoverable, not guaranteed
//!
//! Every step of the chain is optional for a server to support: it may
//! publish no metadata, may not offer open registration, or may withdraw
//! it tomorrow. So discovery failure is **not** an error a person should
//! have to interpret: it means "this server does not do this", and the
//! password field the manual form already offers still works everywhere.
//! [`jmap_signin_offered`] answers exactly the question a setup form asks
//! (whether to show the sign-in button at all) and never fails.
//!
//! The four binding rules the chain runs under live in `astra-oauth`'s
//! discovery and are already proven there: every hop HTTPS with loopback
//! the one exemption, the metadata's issuer matching the issuer asked
//! about, S256 required, and the scopes limited to what the app uses.
//! The [`discovery`] module wires them to the JMAP session's `401` and
//! this module persists what they found.
//!
//! # Why the endpoints are stored rather than re-discovered
//!
//! Discovery runs **once**, at sign-in, and its results persist in
//! `[jmap.oauth]`: the endpoints, the registered `client_id`, and the
//! refresh token. A launch therefore costs no discovery round trips and no
//! re-registration, and a server that later withdraws open registration
//! cannot break an account that already has a client id.

mod discovery;
mod reauth;

pub use reauth::{begin_jmap_reauth, complete_jmap_reauth};

use std::sync::Arc;

use astra_oauth::OAuthClient;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{jmap_base_url, JmapAccount, JmapOAuthGrant};
use crate::{AccountError, CredentialOrigin, GraphTokenSource, TokenSink};
use discovery::{discover_and_register, discover_offer};

/// What [`begin_jmap_login`] returns: the URL to open in the platform auth
/// session, and an opaque handle to pass back to the completion.
pub struct JmapLoginStart {
    /// The authorization URL to open in the platform auth session.
    pub authorization_url: String,
    /// An opaque handle (discovered endpoints + registered client id +
    /// PKCE verifier). **Transient; hold it in memory only.** It carries
    /// the PKCE verifier.
    pub pending: String,
}

impl core::fmt::Debug for JmapLoginStart {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The pending handle carries the PKCE verifier; never surface it.
        f.debug_struct("JmapLoginStart")
            .field("authorization_url", &self.authorization_url)
            .finish_non_exhaustive()
    }
}

/// The transient state carried between begin and complete. Round-tripped
/// through the host as the opaque `pending` handle, and never persisted;
/// it holds the PKCE verifier.
#[derive(Serialize, Deserialize)]
struct PendingJmapLogin {
    /// The address being connected, the identity the grant belongs to.
    email: String,
    /// The JMAP server the account dials.
    base_url: String,
    /// The dynamically registered client id.
    client_id: String,
    /// A client secret, on the rare server that issued one to a public
    /// client.
    client_secret: Option<String>,
    /// The discovered authorization endpoint.
    authorize_endpoint: String,
    /// The discovered token endpoint.
    token_endpoint: String,
    /// The redirect URI this sign-in was registered with.
    redirect_uri: String,
    /// The scopes requested.
    scopes: Vec<String>,
    /// The RFC 8707 resource indicator discovered for this server, carried
    /// across the browser hop so the exchange names the same target the
    /// authorization request did.
    resource: Option<String>,
    /// The issuer the redirect's `iss` must name (RFC 9207), when the
    /// server advertised that it sends one. Carried across the hop because
    /// the check happens on the way back.
    issuer: Option<String>,
    /// The CSRF state the authorization request carried.
    state: String,
    /// The PKCE verifier the exchange sends.
    verifier: String,
}

impl PendingJmapLogin {
    /// Rebuilds the grant this pending login will become, given the
    /// refresh token it was exchanged for, beside the identity it belongs
    /// to. Kept next to the struct so the fields carried across the
    /// browser hop and the fields persisted afterwards cannot drift apart.
    fn into_grant(self, refresh_token: String) -> (String, String, JmapOAuthGrant) {
        let grant = JmapOAuthGrant {
            client_id: self.client_id,
            client_secret: self.client_secret.map(crate::Secret::new),
            refresh_token: crate::Secret::new(refresh_token),
            authorize_endpoint: self.authorize_endpoint,
            token_endpoint: self.token_endpoint,
            redirect_uri: self.redirect_uri,
            scopes: self.scopes,
            resource: self.resource,
            issuer: self.issuer,
        };
        (self.email, self.base_url, grant)
    }
}

/// Whether the JMAP server for `email`/`server_url` offers discoverable
/// OAuth sign-in: the cheap pre-flight a setup form asks before showing
/// the button, so the button never dead-ends in front of the password
/// field that always works.
///
/// Runs the discovery chain short of registration, and offers sign-in only
/// when the metadata names a `registration_endpoint`, because registration
/// is what mints the client id. Never fails: any miss, from an
/// unreachable server to a `401` naming no authorization server, is
/// `false`.
pub async fn jmap_signin_offered(email: &str, server_url: Option<&str>) -> bool {
    let Ok(base_url) = jmap_base_url(email, server_url) else {
        return false;
    };
    discover_offer(&base_url).await.is_some()
}

/// Starts a JMAP OAuth sign-in: discovers the server's authorization
/// server, registers this install as a client, and builds the PKCE
/// authorization URL to open.
///
/// `server_url` may be `None`, in which case it is derived from the email
/// domain by the same rule the manual form uses. `redirect_uri` is the
/// platform's registered custom scheme, whose redirect the host's auth
/// session captures.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] if the server does not support
/// discoverable sign-in at any step. That is an expected outcome, not a
/// defect: the caller falls back to the password field.
pub async fn begin_jmap_login(
    email: &str,
    server_url: Option<&str>,
    redirect_uri: &str,
) -> Result<JmapLoginStart, AccountError> {
    let base_url =
        jmap_base_url(email, server_url).map_err(|err| AccountError::Jmap(err.to_string()))?;
    log::info!("jmap oauth: sign-in requested for the server at {base_url}");
    let discovery::Discovered {
        metadata,
        scopes,
        client,
        resource,
    } = discover_and_register(&base_url, redirect_uri).await?;

    let grant = JmapOAuthGrant {
        client_id: client.client_id,
        client_secret: client.client_secret.map(crate::Secret::new),
        // Not yet issued: a placeholder purely so the provider config can
        // be built from one place. It is never read before the completion
        // replaces it.
        refresh_token: crate::Secret::new(String::new()),
        authorize_endpoint: metadata.authorization_endpoint,
        token_endpoint: metadata.token_endpoint,
        redirect_uri: redirect_uri.to_owned(),
        scopes,
        resource,
        // Only when the server said it sends one: absent then means
        // "nothing to compare", while a `None` here on a server that does
        // send one would skip the RFC 9207 check.
        issuer: metadata
            .issuer_parameter_supported
            .then(|| metadata.issuer.clone()),
    };
    log::info!("jmap oauth: opening the authorization page; awaiting the redirect");
    start_login(email.to_owned(), base_url, &grant)
}

/// Builds the PKCE authorization request for an already-assembled `grant`,
/// and packs everything the completion will need into the opaque `pending`
/// handle.
///
/// The two entry points that need this (a first sign-in, which has just
/// *discovered* the grant, and a re-authentication, which *loaded* it)
/// differ only in where the grant came from, so the authorization half is
/// written once. That is not only tidiness: the fields carried across the
/// browser hop must match the ones the grant is rebuilt from, and
/// duplicating this is exactly how a `redirect_uri` or a `resource`
/// silently stops being replayed.
fn start_login(
    email: String,
    base_url: String,
    grant: &JmapOAuthGrant,
) -> Result<JmapLoginStart, AccountError> {
    let oauth = OAuthClient::new(grant.provider_config())
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    // The address is known, so target it rather than making the person
    // pick.
    let request = oauth.begin(Some(&email));
    let pending = PendingJmapLogin {
        email,
        base_url,
        client_id: grant.client_id.clone(),
        client_secret: grant
            .client_secret
            .as_ref()
            .map(|secret| secret.expose().to_owned()),
        authorize_endpoint: grant.authorize_endpoint.clone(),
        token_endpoint: grant.token_endpoint.clone(),
        redirect_uri: grant.redirect_uri.clone(),
        scopes: grant.scopes.clone(),
        resource: grant.resource.clone(),
        issuer: grant.issuer.clone(),
        state: request.state,
        verifier: request.pkce.verifier().to_owned(),
    };
    Ok(JmapLoginStart {
        authorization_url: request.authorization_url,
        pending: serde_json::to_string(&pending)
            .map_err(|err| AccountError::Jmap(err.to_string()))?,
    })
}

/// Completes a JMAP OAuth sign-in: validates the redirect, exchanges the
/// code for tokens, and returns the `[jmap]` config TOML the host stores
/// through the same secure-store path the manual form's result takes.
///
/// The document carries the `[jmap.oauth]` grant and **no** long-lived
/// password: the refresh token is the account's one credential.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] if `pending` is malformed, the user
/// cancelled, the exchange failed, or the server issued no refresh token
/// (without one the account would break within the hour).
pub async fn complete_jmap_login(
    pending: &str,
    callback_url: &str,
    now: OffsetDateTime,
) -> Result<String, AccountError> {
    let section = exchange(pending, callback_url, now).await?;
    let mut root = toml::Table::new();
    root.insert("jmap".into(), section.to_table().into());
    toml::to_string(&root).map_err(|err| AccountError::Jmap(err.to_string()))
}

/// The exchange half of a completed sign-in, shared by the first sign-in
/// and the re-authentication: validates the redirect, exchanges the code,
/// and assembles the section the grant belongs to.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] if `pending` is malformed, the exchange
/// failed, or the server issued no refresh token.
async fn exchange(
    pending: &str,
    callback_url: &str,
    now: OffsetDateTime,
) -> Result<JmapAccount, AccountError> {
    let pending: PendingJmapLogin =
        serde_json::from_str(pending).map_err(|err| AccountError::Jmap(err.to_string()))?;
    let grant_shell = JmapOAuthGrant {
        client_id: pending.client_id.clone(),
        client_secret: pending.client_secret.clone().map(crate::Secret::new),
        // Never sent: the exchange proves the code, not a refresh token.
        refresh_token: crate::Secret::new(String::new()),
        authorize_endpoint: pending.authorize_endpoint.clone(),
        token_endpoint: pending.token_endpoint.clone(),
        redirect_uri: pending.redirect_uri.clone(),
        scopes: pending.scopes.clone(),
        resource: pending.resource.clone(),
        issuer: pending.issuer.clone(),
    };
    let oauth = OAuthClient::new(grant_shell.provider_config())
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    log::info!(
        "jmap oauth: redirect received; exchanging the code at {} (resource indicator: {})",
        pending.token_endpoint,
        pending.resource.as_deref().unwrap_or("(none)"),
    );
    let tokens = oauth
        .complete(callback_url, &pending.state, &pending.verifier, now)
        .await
        .map_err(|err| {
            // The single most valuable line in the whole flow for support:
            // the server's own machine-readable reason. `invalid_target`
            // means the RFC 8707 resource indicator was missing or wrong;
            // `invalid_grant` means the code was stale or replayed. Without
            // this the person only ever sees "signing in didn't work".
            log::warn!("jmap oauth: token exchange FAILED; {err}");
            AccountError::Jmap(err.to_string())
        })?;
    // `offline_access` is always requested, so a refresh token is
    // mandatory; without one the account would stop syncing about an hour
    // after setup, which is far worse to debug than failing here.
    let refresh_token = tokens.refresh_token.ok_or_else(|| {
        log::warn!(
            "jmap oauth: exchange succeeded but the server issued NO refresh token; granted \
             scope(s): [{}]",
            tokens.scope,
        );
        AccountError::Jmap(
            "the server issued no refresh token (offline_access was requested)".to_owned(),
        )
    })?;
    log::info!(
        "jmap oauth: sign-in complete; the access token is valid for {} more minute(s), granted \
         scope(s): [{}]",
        (tokens.expires_at - OffsetDateTime::now_utc()).whole_minutes(),
        tokens.scope,
    );
    let (email, base_url, grant) = pending.into_grant(refresh_token.expose().to_owned());
    Ok(JmapAccount {
        email,
        base_url,
        password: None,
        oauth: Some(grant),
    })
}

/// Builds the shared, self-refreshing token source for a JMAP account's
/// stored grant: the JMAP parallel of
/// [`google_token_source`](crate::google_token_source), reusing the same
/// provider-neutral source the Microsoft and Google accounts dial
/// through, so the single-flight refresh and the replay cool-downs are
/// inherited rather than reimplemented.
///
/// `sink` receives a rotated refresh token for re-persistence. On a
/// ratcheting JMAP server the rotation write is what keeps the account
/// alive at the next launch, so a real host always passes one; `None`
/// keeps the rotation in memory only.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] when the section stores no grant (a
/// password account has no token to mint), or the OAuth HTTP client
/// cannot be built.
pub fn jmap_token_source(
    section: &JmapAccount,
    account: engine_api::AccountId,
    sink: Option<Arc<dyn TokenSink>>,
    origin: CredentialOrigin,
) -> Result<Arc<GraphTokenSource>, AccountError> {
    let grant = section
        .oauth
        .as_ref()
        .ok_or_else(|| AccountError::Jmap("the [jmap] section stores no oauth grant".to_owned()))?;
    let oauth = OAuthClient::new(grant.provider_config())
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    Ok(GraphTokenSource::from_parts(
        oauth,
        account,
        grant.refresh_token.expose().to_owned(),
        sink,
        "jmap",
        origin,
    ))
}

#[cfg(test)]
#[path = "../jmap_oauth_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../jmap_oauth_tests.rs"]
mod tests;
