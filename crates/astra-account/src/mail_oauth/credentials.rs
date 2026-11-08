// SPDX-License-Identifier: GPL-3.0-only
//! What an OAuth mail account presents to its servers: the credential
//! source its connections are dialled with.
//!
//! The engine asks a [`CredentialSource`] once per IMAP connection the
//! account dials and once per SMTP submission, and once more after a
//! refusal. This module hands the shared [`GraphTokenSource`] to that
//! trait: every dial is answered with a token valid at that moment, and a
//! refused token is replaced through the same single-flight refresh, so
//! the engine's one-renewal retry never races a second refresh.

use std::sync::Arc;

use async_trait::async_trait;
use engine_api::AccountId;
use engine_provider::ProviderError;
use provider_imap::{CredentialSource, Credentials};

use crate::mail_oauth::grant::MailOauthGrant;
use crate::{AccountError, CredentialOrigin, GraphTokenSource, ImapAccount, TokenSink};

/// The token-bearing credential source of a grant-carrying mail account.
///
/// The username is the account's login name (the address the token was
/// issued for), which is what both SASL mechanisms carry as the
/// authorization identity.
#[derive(Debug)]
struct MailOauthCredentialSource {
    /// The SASL authorization identity: the account address.
    username: String,
    /// The shared token source every dial of this account asks.
    tokens: Arc<GraphTokenSource>,
}

impl MailOauthCredentialSource {
    /// The credential for `token`, in the engine's OAuth shape.
    fn credential(&self, token: String) -> Credentials {
        Credentials::oauth2(self.username.clone(), token)
    }

    /// Maps a token-source failure to the engine's error class: a
    /// refused grant is an authentication failure (the host must
    /// re-auth), and everything else, an unreachable token endpoint
    /// among them, is retryable.
    fn refused(err: AccountError) -> ProviderError {
        match err {
            AccountError::SigninRejected(detail) => ProviderError::authentication(detail),
            other => ProviderError::retryable(other.to_string()),
        }
    }
}

#[async_trait]
impl CredentialSource for MailOauthCredentialSource {
    async fn credentials(&self) -> Result<Credentials, ProviderError> {
        let token = self.tokens.access_token().await.map_err(Self::refused)?;
        Ok(self.credential(token))
    }

    async fn renew(&self, refused: &Credentials) -> Result<Option<Credentials>, ProviderError> {
        // Only a token of ours is worth replacing: a password is never
        // this source's credential, and there is nothing to present for
        // one. The shared source drops the refused token from its cache
        // and refreshes once; a caller behind the first renewal finds
        // the replacement cached, which is what keeps this at one
        // refresh per refusal however often the engine asks.
        let Credentials::OAuth2 { access_token, .. } = refused else {
            return Ok(None);
        };
        let token = self
            .tokens
            .access_token_replacing(access_token)
            .await
            .map_err(Self::refused)?;
        Ok(Some(self.credential(token)))
    }
}

/// Builds the shared, self-refreshing token source for a mail account's
/// stored grant: the mail parallel of
/// [`jmap_token_source`](crate::jmap_token_source), reusing the same
/// provider-neutral source the Microsoft, Google, and JMAP accounts dial
/// through, so the single-flight refresh and the replay cool-downs are
/// inherited rather than reimplemented.
///
/// `sink` receives a rotated refresh token for re-persistence; `None`
/// keeps the rotation in memory only.
///
/// # Errors
///
/// Returns [`AccountError::MailOauth`] when the OAuth HTTP client cannot
/// be built.
pub fn imap_oauth_token_source(
    grant: &MailOauthGrant,
    account: AccountId,
    sink: Option<Arc<dyn TokenSink>>,
    origin: CredentialOrigin,
) -> Result<Arc<GraphTokenSource>, AccountError> {
    let oauth = astra_oauth::OAuthClient::new(grant.provider_config())
        .map_err(|err| AccountError::MailOauth(err.to_string()))?;
    Ok(GraphTokenSource::from_parts(
        oauth,
        account,
        grant.refresh_token.expose().to_owned(),
        sink,
        "imap",
        origin,
    ))
}

/// The credential source for a grant-carrying `[imap]` section: every
/// dial asks `tokens`, and a refused token is renewed once through the
/// same shared source. The section supplies the SASL username; the
/// account stores no password on this path.
///
/// # Errors
///
/// Returns [`AccountError::MissingCredential`] when the section stores no
/// grant, which the password form's source answers instead.
pub fn imap_oauth_credential_source(
    section: &ImapAccount,
    tokens: Arc<GraphTokenSource>,
) -> Result<Arc<dyn CredentialSource>, AccountError> {
    if section.oauth.is_none() {
        return Err(AccountError::MissingCredential(
            "the account stores no oauth grant",
        ));
    }
    Ok(Arc::new(MailOauthCredentialSource {
        username: section.username.clone(),
        tokens,
    }))
}

#[cfg(test)]
#[path = "../mail_oauth_credentials_tests.rs"]
mod tests;
