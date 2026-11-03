// SPDX-License-Identifier: GPL-3.0-only
//! Where a mail account's OAuth authorization server is learned from:
//! the issuer chain, and the registration cache that keeps a sign-in
//! retry from minting orphaned clients.
//!
//! Two channels, both the provider describing itself, never a third
//! party: the `<oAuth2><issuer>` of the provider's own autoconfig (the
//! autodetect crate parses and trust-filters it before the field
//! crosses here), and failing that the RFC 8414 well-known of the email
//! domain and its registrable domain. Every miss is soft: the answer is
//! `None` and the setup path offers the password field, which works
//! everywhere.

use std::sync::{Mutex, OnceLock};

use astra_oauth::{AuthServerMetadata, RegisteredClient};

use crate::AccountError;

/// The client name shown on the provider's consent screen. The product
/// name, per the brand rule: this is user-facing copy on someone else's
/// page.
const CLIENT_NAME: &str = "Astra Mail";

/// Registrations already made this session, keyed by
/// `(issuer, redirect_uri)`.
///
/// Dynamic registration mints a **new** client id every time it is
/// called, so without this every cancelled or failed attempt would leave
/// another orphaned client registration behind on the person's account.
/// Session-scoped on purpose: once a sign-in completes, the client id
/// persists with the account and no further registration ever happens
/// for it.
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

/// Finds the authorization server a mail account can register with, or
/// `None` on every miss.
///
/// The candidates, in order: the issuer the provider's own autoconfig
/// named (as parsed and trust-filtered upstream; the ISPDB's block and
/// any untrusted hop are dropped before the field crosses), then the
/// RFC 8414 well-known of the email domain, then of its registrable
/// domain. A candidate counts only when its metadata advertises a
/// `registration_endpoint`: that is what mints this install's client id,
/// and a server without one has told us it admits only applications it
/// registered in advance.
///
/// Returns the **metadata**, not just the issuer, because the sign-in
/// flow that goes on to register needs every field of it and re-fetching
/// would be a second round trip that could disagree with the first.
pub async fn mail_oauth_issuer(
    email_domain: &str,
    detected: Option<&astra_autodetect::DetectedMailSettings>,
) -> Option<AuthServerMetadata> {
    let http = astra_oauth::discovery_client_with(&crate::setup_trust_policy()).ok()?;
    for issuer in issuer_candidates(email_domain, detected) {
        match astra_oauth::discover_auth_server(&http, &issuer).await {
            Ok(metadata) if metadata.registration_endpoint.is_some() => {
                log::info!("mail oauth: {issuer} offers open registration; sign-in can be offered");
                return Some(metadata);
            }
            Ok(metadata) => log::info!(
                "mail oauth: {} publishes metadata but no registration endpoint",
                metadata.issuer,
            ),
            Err(err) => log::debug!("mail oauth: no authorization server at {issuer}; {err}"),
        }
    }
    None
}

/// The registrable ("base") domain of `host` per the Public Suffix List,
/// e.g. `mail.example.co.uk` to `example.co.uk`.
///
/// Public-suffix-aware on purpose, and the same library the autodetect
/// crate's MX derivation already uses: the naive last-two-labels cut
/// answers `co.uk` for a `mail.example.co.uk`, which is a public suffix
/// itself, and offering its well-known is a probe of a domain nobody
/// runs. `None` for anything the list cannot answer (a single label, an
/// IP literal, an empty string), and the candidate is simply skipped.
fn registrable_domain(host: &str) -> Option<String> {
    let host = host.trim();
    // An IP literal is not a domain, and the list would cut it as one:
    // `127.0.0.1` reads as the domain `0.1`.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    psl::domain_str(host).map(str::to_owned)
}

/// The issuers to try, in order: the one the provider's own autoconfig
/// named, then the email domain, then its registrable domain, each as
/// the HTTPS URL an RFC 8414 identifier is, and none twice.
fn issuer_candidates(
    email_domain: &str,
    detected: Option<&astra_autodetect::DetectedMailSettings>,
) -> Vec<String> {
    let mut candidates: Vec<String> = detected
        .and_then(|settings| settings.oauth_issuer.clone())
        .into_iter()
        .collect();
    let domain = email_domain.trim().to_ascii_lowercase();
    let mut add = |domain: &str| {
        if domain.is_empty() {
            return;
        }
        let candidate = format!("https://{domain}");
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };
    add(&domain);
    if let Some(registrable) = registrable_domain(&domain) {
        add(&registrable);
    }
    candidates
}

/// The detection stand-in a host builds from the issuer its
/// recommendation view carried: the wizard holds the recommendation, not
/// the raw [`DetectedMailSettings`] the chain read, and the chain's only
/// use of the detection result is the issuer field. The stand-in carries
/// that field; every other field is inert filler the struct requires and
/// nothing on this path reads.
#[must_use]
pub fn detected_issuer(issuer: Option<String>) -> astra_autodetect::DetectedMailSettings {
    astra_autodetect::DetectedMailSettings {
        incoming: Vec::new(),
        outgoing: Vec::new(),
        // The recommendation is the trust-filtered product of the run
        // that named the issuer: only a provider describing itself over
        // HTTPS may name one, and that fact crossed when the field did.
        is_trusted: true,
        source: astra_autodetect::Source {
            kind: astra_autodetect::SourceKind::Autoconfig,
            url: String::new(),
        },
        oauth_issuer: issuer,
        dav: astra_autodetect::DetectedDav::default(),
    }
}

/// Registers this install as a client of `metadata`'s server (RFC 7591),
/// reusing the id already minted this session for the same issuer and
/// redirect.
///
/// # Errors
///
/// Returns [`AccountError::MailOauth`] when the registration is refused
/// or cannot be sent. The caller answers with the password field.
pub async fn register_mail_client(
    metadata: &AuthServerMetadata,
    redirect_uri: &str,
) -> Result<RegisteredClient, AccountError> {
    if let Some(client) = cached_registration(&metadata.issuer, redirect_uri) {
        log::info!(
            "mail oauth: reusing client id {} registered earlier this session",
            client.client_id,
        );
        return Ok(client);
    }
    let http = astra_oauth::discovery_client_with(&crate::setup_trust_policy())
        .map_err(|err| AccountError::MailOauth(err.to_string()))?;
    let scopes = astra_oauth::select_scopes(metadata);
    let client = astra_oauth::register_client(&http, metadata, CLIENT_NAME, redirect_uri, &scopes)
        .await
        .map_err(|err| {
            log::info!(
                "mail oauth: registration refused by {}; {err}",
                metadata.issuer
            );
            AccountError::MailOauth(format!("registering with {}: {err}", metadata.issuer))
        })?;
    log::info!(
        "mail oauth: registered client id {} with {} (server issued a secret: {})",
        client.client_id,
        metadata.issuer,
        client.client_secret.is_some(),
    );
    cache_registration(&metadata.issuer, redirect_uri, &client);
    Ok(client)
}

#[cfg(test)]
#[path = "../mail_oauth_issuer_tests.rs"]
mod tests;
