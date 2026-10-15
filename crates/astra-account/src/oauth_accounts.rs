// SPDX-License-Identifier: GPL-3.0-only
//! The OAuth provider sections of a stored account document: `[graph]`
//! for a Microsoft account and `[google]` for a Google one. The app
//! registration each family's access tokens mint through, the
//! long-lived refresh token those mints start from, and the scope set
//! the server last granted.
//!
//! Each section builds the `astra-oauth` provider config its
//! registration names, which is the whole difference between them at
//! this layer: Microsoft names a tenant and needs no secret, while
//! Google's endpoints are fixed and the Desktop client's non-confidential
//! secret rides both grant types.

use astra_oauth::OAuthProviderConfig;
use serde::Deserialize;

use crate::Secret;

/// The `[graph]` section of a stored Microsoft account: the app
/// registration its access tokens mint through, the long-lived refresh
/// token those mints start from, and the scope set the server last
/// granted for it.
///
/// The short-lived access token is never stored; the token source mints
/// one on demand at connect. Like the rest of the config, the refresh
/// token redacts itself in `Debug`, so a config holding it is safe to
/// log.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphAccount {
    /// The account's own email address (Graph `/me`), which names the
    /// account.
    pub email: String,
    /// The registered application (client) id of the product's Azure app.
    /// Not a secret.
    pub client_id: String,
    /// The tenant the app authenticates against (`common`,
    /// `organizations`, `consumers`, or a tenant id).
    pub tenant: String,
    /// The redirect URI the OAuth flow returns to (a custom scheme the
    /// shell captures).
    pub redirect_uri: String,
    /// The delegated scopes the sign-in asks for.
    pub scopes: Vec<String>,
    /// The OAuth refresh token: the section's only secret.
    pub refresh_token: Secret,
    /// The scopes the server actually granted, which may be fewer than
    /// were asked for: a person can untick one on the consent screen, or
    /// withdraw one later. A copy of the last token response that named
    /// them, written at sign-in and overwritten by any refresh that names
    /// a different set. `None` for a grant stored before this was
    /// recorded, which is read as withholding nothing until a refresh
    /// says otherwise.
    #[serde(default)]
    pub granted_scopes: Option<Vec<String>>,
}

impl GraphAccount {
    /// Builds the `astra-oauth` provider config this section's
    /// registration names, so the token source can refresh its access
    /// token.
    #[must_use]
    pub fn provider_config(&self) -> OAuthProviderConfig {
        let scopes: Vec<&str> = self.scopes.iter().map(String::as_str).collect();
        OAuthProviderConfig::microsoft(
            self.client_id.clone(),
            &self.tenant,
            self.redirect_uri.clone(),
            &scopes,
        )
    }
}

/// The `[google]` section of a stored Google account: the app
/// registration its access tokens mint through, the long-lived refresh
/// token those mints start from, and the scope set the server last
/// granted for it.
///
/// The section's Google delta over `[graph]` is the Desktop client's
/// **non-confidential secret**: Google's token endpoint requires it on
/// the code exchange AND the refresh, so it persists here under
/// [`Secret`] (this layer never logs it) for a launch-later refresh to
/// send again. Google has no per-tenant authority and issues the
/// refresh token from request parameters rather than a scope, so the
/// section names neither a tenant nor an `offline_access` scope. The
/// short-lived access token is never stored; the token source mints one
/// on demand at connect.
#[derive(Debug, Clone, Deserialize)]
pub struct GoogleAccount {
    /// The account's own email address (the userinfo read), which names
    /// the account.
    pub email: String,
    /// The registered OAuth client id of the product's Google Cloud
    /// app. Not a secret.
    pub client_id: String,
    /// The non-confidential Google Desktop client's secret, when this
    /// account was connected through the loopback flow. `None` leaves
    /// the key out of the stored form entirely. Google documents the
    /// Desktop secret as not treated as a secret; it is wrapped in
    /// [`Secret`] anyway because this layer logs nothing it carries.
    #[serde(default)]
    pub client_secret: Option<Secret>,
    /// The redirect URI the OAuth flow returns to (the loopback URL
    /// whose port the sign-in bound).
    pub redirect_uri: String,
    /// The delegated scopes the sign-in asks for.
    pub scopes: Vec<String>,
    /// The OAuth refresh token: the section's stored grant, minting
    /// access tokens at connect time and never the reverse.
    pub refresh_token: Secret,
    /// The scopes the server actually granted, which may be fewer than
    /// were asked for: a person can untick one on the consent screen, or
    /// withdraw one later. A copy of the last token response that named
    /// them, written at sign-in and overwritten by any refresh that
    /// names a different set. `None` for a grant stored before this was
    /// recorded, which is read as withholding nothing until a refresh
    /// says otherwise.
    #[serde(default)]
    pub granted_scopes: Option<Vec<String>>,
}

impl GoogleAccount {
    /// Builds the `astra-oauth` provider config this section's
    /// registration names, so the token source can refresh its access
    /// token: the fixed Google endpoints plus the client registration,
    /// with the Desktop secret riding both grant types.
    #[must_use]
    pub fn provider_config(&self) -> OAuthProviderConfig {
        let scopes: Vec<&str> = self.scopes.iter().map(String::as_str).collect();
        OAuthProviderConfig::google(
            self.client_id.clone(),
            self.client_secret
                .as_ref()
                .map(|secret| secret.expose().to_owned()),
            self.redirect_uri.clone(),
            &scopes,
        )
    }
}
