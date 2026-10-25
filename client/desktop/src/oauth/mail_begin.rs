// SPDX-License-Identifier: GPL-3.0-only
//! The mail sign-in's route walk: what the begin seam runs to mint one
//! authorization URL. A registration this build already holds beats
//! discovery, because the provider has told us in advance which client
//! we are; otherwise the issuer chain's metadata and RFC 7591
//! registration mint the client, and the PKCE request rides whichever
//! route answered. Split from the command surface for the file cap; the
//! command surface's tests pin the halves over the seam, and the route
//! itself is the issuer chain's own proven ground.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use astra_account::{AccountConfig, AccountError};
use astra_oauth::{self as oauth, AuthStyle, OAuthProviderConfig};

use super::mail_add::{MailCompletion, PendingMailLogin};
use super::session::OAUTH_REDIRECT_URI;

/// What a begun mail sign-in starts with: the URL to open and the opaque
/// handle the completion spends.
pub(crate) struct MailLoginStart {
    /// The authorization URL to open in the system browser.
    pub(crate) authorization_url: String,
    /// The opaque pending handle, minted here and spent by the
    /// completion. Transient; it carries the PKCE verifier.
    pub(crate) pending: String,
}

/// The begin seam: the built config and the detection's issuer hint in,
/// the minted start out. The production seam walks the route (the static
/// table, else the discovered issuer), registers this install, and mints
/// the PKCE request.
pub(crate) type MailBegin = Arc<
    dyn Fn(
            AccountConfig,
            Option<String>,
        ) -> Pin<Box<dyn Future<Output = Result<MailLoginStart, AccountError>> + Send>>
        + Send
        + Sync,
>;

/// The halves one begun mail sign-in runs over.
pub(crate) struct MailFlowHalves {
    /// The begin seam: the config and issuer hint in, the start out.
    pub(crate) begin: MailBegin,
    /// The completion seam: the pending handle and the raw redirect in,
    /// the outcome out.
    pub(crate) complete: MailCompletion,
    /// How long the route walk and the registration may run before the
    /// begin refuses.
    pub(crate) bound: Duration,
}

/// The domain part of `email`, or the empty string when there is none.
fn email_domain(email: &str) -> &str {
    email
        .trim()
        .rsplit_once('@')
        .map_or("", |(_, domain)| domain.trim())
}

/// The production begin: the static table's registration for the dialled
/// host, else the discovered issuer's metadata and RFC 7591 registration,
/// then the PKCE request over whichever route answered. The pending
/// handle packs the account's own server sections and the certificate
/// the person accepted, so the completion's document needs no second
/// look at the step's fields.
async fn begin_mail_login(
    config: AccountConfig,
    oauth_issuer: Option<String>,
) -> Result<MailLoginStart, AccountError> {
    let imap = config
        .imap
        .as_ref()
        .ok_or_else(|| AccountError::MailOauth("the setup names no imap server".to_owned()))?;
    let host = imap.server_name.trim().to_ascii_lowercase();
    let email = imap.username.trim();
    // The route: a registration this build already holds beats
    // discovery, because the provider has told us in advance which
    // client we are; an entry whose client id is absent is not a route.
    let tabled = oauth::provider_for_host(&oauth::static_mail_providers(), &host)
        .filter(|provider| !provider.client_id.is_empty());
    let (provider, issuer) = if let Some(provider) = tabled {
        log::info!(
            "mail oauth: using this build's registration for {} at {}",
            provider.label,
            provider.issuer
        );
        (
            OAuthProviderConfig {
                authorize_endpoint: provider.authorize_endpoint.to_owned(),
                token_endpoint: provider.token_endpoint.to_owned(),
                client_id: provider.client_id.clone(),
                client_secret: provider.client_secret.clone(),
                redirect_uri: OAUTH_REDIRECT_URI.to_owned(),
                scopes: provider
                    .scopes
                    .iter()
                    .map(|scope| (*scope).to_owned())
                    .collect(),
                // No `resource`: the RFC 8707 indicator is the JMAP
                // leg's, and this grant never sends one.
                resource: None,
                // The static entry carries the issuer its own
                // metadata claims, but not that the server sends
                // `iss` on the response; absent means nothing to
                // compare, which is the pre-RFC-9207 status quo.
                expected_issuer: None,
                style: AuthStyle::Discovered,
            },
            None,
        )
    } else {
        let detected = astra_account::detected_issuer(oauth_issuer);
        let Some(metadata) =
            astra_account::mail_oauth_issuer(email_domain(email), Some(&detected)).await
        else {
            return Err(AccountError::MailOauth(format!(
                "no authorization server with open registration answered for {host}"
            )));
        };
        let scopes = oauth::select_scopes(&metadata);
        let client = astra_account::register_mail_client(&metadata, OAUTH_REDIRECT_URI).await?;
        // Only when the server said it sends one: absent then means
        // "nothing to compare", while a None here on a server that
        // does send one would skip the RFC 9207 check.
        let issuer = metadata
            .issuer_parameter_supported
            .then(|| metadata.issuer.clone());
        (
            OAuthProviderConfig {
                authorize_endpoint: metadata.authorization_endpoint.clone(),
                token_endpoint: metadata.token_endpoint.clone(),
                client_id: client.client_id.clone(),
                client_secret: client.client_secret.clone(),
                redirect_uri: OAUTH_REDIRECT_URI.to_owned(),
                scopes,
                resource: None,
                expected_issuer: issuer.clone(),
                style: AuthStyle::Discovered,
            },
            issuer,
        )
    };
    let client = oauth::OAuthClient::new(provider.clone())
        .map_err(|err| AccountError::MailOauth(err.to_string()))?;
    // The address is known, so target it rather than making the person
    // pick.
    let request = client.begin(Some(email));
    let pending = PendingMailLogin {
        email: email.to_owned(),
        imap_addr: imap.addr.clone(),
        server_name: imap.server_name.clone(),
        security: imap.security,
        accepted: config.certificate_exceptions.first().cloned(),
        client_id: provider.client_id,
        client_secret: provider.client_secret,
        authorize_endpoint: provider.authorize_endpoint,
        token_endpoint: provider.token_endpoint,
        redirect_uri: provider.redirect_uri,
        scopes: provider.scopes,
        issuer,
        state: request.state,
        verifier: request.pkce.verifier().to_owned(),
    };
    log::info!("mail oauth: opening the authorization page for {email}; awaiting the redirect");
    Ok(MailLoginStart {
        authorization_url: request.authorization_url,
        pending: serde_json::to_string(&pending)
            .map_err(|err| AccountError::MailOauth(err.to_string()))?,
    })
}

/// The production begin seam.
#[must_use]
pub(crate) fn production_mail_begin() -> MailBegin {
    Arc::new(|config, oauth_issuer| {
        Box::pin(async move { begin_mail_login(config, oauth_issuer).await })
    })
}
