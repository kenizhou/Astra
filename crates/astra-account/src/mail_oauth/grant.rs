// SPDX-License-Identifier: GPL-3.0-only
//! The `[imap.oauth]` block of a browser-signed-in mail account:
//! everything needed to mint a fresh access token at launch without
//! repeating discovery, modelled on the JMAP grant.
//!
//! A grant-carrying account stores this block and **no** long-lived
//! password: the refresh token is the account's one credential, and the
//! engine presents it over SASL on both the IMAP and the SMTP leg.

use serde::Deserialize;

use crate::Secret;

/// The `[imap.oauth]` block of a browser-signed-in mail account: the
/// JMAP grant's shape model, minus the RFC 8707 `resource` an IMAP
/// account deliberately never sends.
///
/// The optional fields (`client_secret`, `issuer`) carry
/// `#[serde(default)]` so a document written before the field existed
/// still loads: absent means "nothing to send / nothing to compare",
/// which is the pre-existing behaviour rather than a fault.
#[derive(Debug, Clone, Deserialize)]
pub struct MailOauthGrant {
    /// The client identifier, from dynamic registration (RFC 7591) at
    /// sign-in, or from this build's static table. Not a secret.
    /// Persisted so a launch never re-registers.
    pub client_id: String,
    /// A client secret, on the rare server that issued one to a client
    /// that asked to be public. Sent on the token exchange and the
    /// refresh when present. Not confidential; an installed app cannot
    /// keep a secret; PKCE is the real protection.
    #[serde(default)]
    pub client_secret: Option<Secret>,
    /// The long-lived refresh token. Rotated in place when the server
    /// issues a new one, and re-persisted through the host's
    /// [`TokenSink`](crate::TokenSink): a ratcheting server revokes the
    /// whole grant over a replayed token, so a rotation nobody stored
    /// kills the account at the next launch.
    pub refresh_token: Secret,
    /// The discovered authorization endpoint, kept so a re-consent needs
    /// no re-discovery.
    pub authorize_endpoint: String,
    /// The discovered token endpoint; where every refresh goes.
    pub token_endpoint: String,
    /// The redirect URI this grant was registered with. Must be replayed
    /// verbatim on refresh and on any re-authorisation, or the server
    /// rejects the grant.
    pub redirect_uri: String,
    /// The scopes granted, re-sent on refresh so the grant is re-issued
    /// for the same set.
    pub scopes: Vec<String>,
    /// The issuer to check a re-authorisation's `iss` against
    /// (RFC 9207), when the server advertised that it sends one. `None`
    /// for a server that does not: absent means "nothing to compare",
    /// which is the pre-RFC-9207 status quo and not a reason to break a
    /// working account.
    #[serde(default)]
    pub issuer: Option<String>,
}

impl MailOauthGrant {
    /// The OAuth client config this grant refreshes through: its own
    /// endpoints, its registered client id, and
    /// [`AuthStyle::Discovered`](astra_oauth::AuthStyle::Discovered),
    /// which sends only the parameters RFC 6749 and RFC 7636 define,
    /// because this server's documentation is unread and vendor
    /// extensions it may reject are not guessed at.
    ///
    /// No `resource`: the RFC 8707 indicator is the JMAP leg's, and this
    /// grant never sends one.
    pub(crate) fn provider_config(&self) -> astra_oauth::OAuthProviderConfig {
        astra_oauth::OAuthProviderConfig {
            authorize_endpoint: self.authorize_endpoint.clone(),
            token_endpoint: self.token_endpoint.clone(),
            client_id: self.client_id.clone(),
            client_secret: self
                .client_secret
                .as_ref()
                .map(|secret| secret.expose().to_owned()),
            redirect_uri: self.redirect_uri.clone(),
            scopes: self.scopes.clone(),
            resource: None,
            expected_issuer: self.issuer.clone(),
            style: astra_oauth::AuthStyle::Discovered,
        }
    }

    /// The grant as the `[imap.oauth]` table the stored document carries:
    /// the inverse of deserialisation, manual so [`Secret`] stays
    /// non-serialisable outside this boundary.
    pub(crate) fn to_table(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("client_id".into(), self.client_id.clone().into());
        if let Some(secret) = &self.client_secret {
            table.insert("client_secret".into(), secret.expose().to_owned().into());
        }
        table.insert(
            "refresh_token".into(),
            self.refresh_token.expose().to_owned().into(),
        );
        table.insert(
            "authorize_endpoint".into(),
            self.authorize_endpoint.clone().into(),
        );
        table.insert("token_endpoint".into(), self.token_endpoint.clone().into());
        table.insert("redirect_uri".into(), self.redirect_uri.clone().into());
        table.insert(
            "scopes".into(),
            toml::Value::Array(self.scopes.iter().cloned().map(Into::into).collect()),
        );
        if let Some(issuer) = &self.issuer {
            table.insert("issuer".into(), issuer.clone().into());
        }
        table
    }
}

#[cfg(test)]
#[path = "../mail_oauth_grant_tests.rs"]
mod tests;
