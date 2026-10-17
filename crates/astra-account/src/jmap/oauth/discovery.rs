// SPDX-License-Identifier: GPL-3.0-only
//! The RFC 9728, RFC 8414, RFC 7591 chain behind
//! [`begin_jmap_login`](super::begin_jmap_login) and the offer probe,
//! split from the module root so both stay under the 500-line cap.
//!
//! Everything here logs. That is the point: this runs against servers
//! nobody on the team has seen, so a support request has to be answerable
//! from the attached log alone; how far it got, and what the server said.
//! All of it is server configuration (URLs, scope names, the issued client
//! id); never the person's address, secret, or token.

use std::sync::{Mutex, OnceLock};

use astra_oauth::{
    discover_auth_server, discover_protected_resource, discovery_client_with, grants_mail_access,
    register_client, select_scopes, RegisteredClient,
};

use super::AccountError;

/// The client name shown on the provider's consent screen. The product
/// name, per the brand rule: this is user-facing copy on someone else's
/// page.
const CLIENT_NAME: &str = "Astra Mail";

/// The JMAP session resource path (RFC 8620 section 2), which is also the
/// protected resource whose `401` names the authorization server.
const SESSION_PATH: &str = "/.well-known/jmap";

/// Registrations already made this session, keyed by
/// `(issuer, redirect_uri)`.
///
/// Dynamic registration mints a **new** client id every time it is called,
/// so without this every cancelled or failed attempt would leave another
/// orphaned client registration behind on the person's account; three taps
/// of a flaky sign-in, three registrations. Reusing the one already held
/// is also simply correct: same software, same install, same server.
///
/// Session-scoped on purpose. Once a sign-in *completes*, the client id
/// persists with the account and no further registration ever happens for
/// it, so the only case left to cover is repeated attempts before that,
/// which is exactly the case that leaked.
static REGISTRATIONS: OnceLock<Mutex<Vec<(String, String, RegisteredClient)>>> = OnceLock::new();

/// The client already registered for `issuer` + `redirect_uri` this
/// session, if any.
fn cached_registration(issuer: &str, redirect_uri: &str) -> Option<RegisteredClient> {
    let cache = REGISTRATIONS.get_or_init(|| Mutex::new(Vec::new()));
    let cache = cache.lock().ok()?;
    cache
        .iter()
        .find(|(cached_issuer, cached_redirect, _)| {
            cached_issuer == issuer && cached_redirect == redirect_uri
        })
        .map(|(_, _, client)| client.clone())
}

/// Remembers `client` for `issuer` + `redirect_uri` so a retry reuses it.
fn cache_registration(issuer: &str, redirect_uri: &str, client: &RegisteredClient) {
    let cache = REGISTRATIONS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut cache) = cache.lock() {
        cache.push((issuer.to_owned(), redirect_uri.to_owned(), client.clone()));
    }
}

/// Everything the discovery chain produced, ready to build an
/// authorization request from.
pub(super) struct Discovered {
    pub(super) metadata: astra_oauth::AuthServerMetadata,
    pub(super) scopes: Vec<String>,
    pub(super) client: RegisteredClient,
    /// The RFC 8707 resource indicator the server published, if any.
    pub(super) resource: Option<String>,
}

/// The discovery chain's answer when the server at `base_url` offers
/// sign-in: its authorization-server metadata. `None` for every miss.
pub(super) async fn discover_offer(base_url: &str) -> Option<astra_oauth::AuthServerMetadata> {
    let http = discovery_client_with(&crate::setup_trust_policy()).ok()?;
    let session_url = format!("{}{SESSION_PATH}", base_url.trim_end_matches('/'));
    let protected = match discover_protected_resource(&http, &session_url).await {
        Ok(protected) => protected,
        Err(err) => {
            log::debug!("jmap oauth: no sign-in offered for the server at {base_url}; {err}");
            return None;
        }
    };
    match discover_auth_server(&http, &protected.issuer).await {
        Ok(metadata) => {
            // Registration is what actually mints our client id, so a
            // server that advertises no registration endpoint cannot be
            // offered sign-in.
            let offered = metadata.registration_endpoint.is_some();
            log::debug!(
                "jmap oauth: {} advertises sign-in: {offered}",
                protected.issuer,
            );
            offered.then_some(metadata)
        }
        Err(err) => {
            log::debug!(
                "jmap oauth: no sign-in offered by {}; {err}",
                protected.issuer,
            );
            None
        }
    }
}

/// Runs the RFC 9728, RFC 8414, RFC 7591 chain for the JMAP server at
/// `base_url`.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] for every miss along the chain (the
/// expected outcome a caller answers with the password field) and,
/// distinctively, when the server's scopes include no mail capability: a
/// grant that mints no mail access would hand the account an empty
/// mailbox.
pub(super) async fn discover_and_register(
    base_url: &str,
    redirect_uri: &str,
) -> Result<Discovered, AccountError> {
    let http = discovery_client_with(&crate::setup_trust_policy())
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    let session_url = format!("{}{SESSION_PATH}", base_url.trim_end_matches('/'));

    log::info!("jmap oauth: probing {session_url} for resource metadata (RFC 9728)");
    let protected = discover_protected_resource(&http, &session_url)
        .await
        .map_err(|err| discovery_unavailable(&err))?;
    log::info!(
        "jmap oauth: issuer {}, resource indicator {}",
        protected.issuer,
        protected.resource.as_deref().unwrap_or("(none published)"),
    );

    log::info!(
        "jmap oauth: reading authorization-server metadata for {} (RFC 8414)",
        protected.issuer,
    );
    let metadata = discover_auth_server(&http, &protected.issuer)
        .await
        .map_err(|err| discovery_unavailable(&err))?;
    log::info!(
        "jmap oauth: authorize {}, token {}, register {}",
        metadata.authorization_endpoint,
        metadata.token_endpoint,
        metadata
            .registration_endpoint
            .as_deref()
            .unwrap_or("(none; sign-in cannot be offered)"),
    );

    let scopes = select_scopes(&metadata);
    log::info!(
        "jmap oauth: server offers {} scope(s); requesting [{}]",
        metadata.scopes_supported.len(),
        scopes.join(", "),
    );
    if !grants_mail_access(&metadata, &scopes) {
        log::warn!(
            "jmap oauth: no mail scope among [{}]; the password field still works",
            metadata.scopes_supported.join(", "),
        );
        return Err(AccountError::Jmap(
            "this server's sign-in does not offer mail access".to_owned(),
        ));
    }

    let client = if let Some(client) = cached_registration(&metadata.issuer, redirect_uri) {
        log::info!(
            "jmap oauth: reusing client id {} registered earlier this session",
            client.client_id,
        );
        client
    } else {
        log::info!("jmap oauth: registering this install as a client (RFC 7591)");
        let client = register_client(&http, &metadata, CLIENT_NAME, redirect_uri, &scopes)
            .await
            .map_err(|err| discovery_unavailable(&err))?;
        log::info!(
            "jmap oauth: registered client id {} (server issued a secret: {})",
            client.client_id,
            client.client_secret.is_some(),
        );
        cache_registration(&metadata.issuer, redirect_uri, &client);
        client
    };
    Ok(Discovered {
        metadata,
        scopes,
        client,
        resource: protected.resource,
    })
}

/// Maps a discovery failure to the account error. Every variant means the
/// same thing to a person (this server does not offer sign-in, use the
/// password field), so they share one message, with the specific cause
/// kept in the diagnostic log rather than shown.
fn discovery_unavailable(err: &astra_oauth::DiscoveryError) -> AccountError {
    log::info!("jmap oauth: discovery unavailable ({err}); the password field still works");
    AccountError::Jmap(format!("this server does not support signing in: {err}"))
}

#[cfg(test)]
#[path = "../../jmap_discovery_tests.rs"]
mod tests;
