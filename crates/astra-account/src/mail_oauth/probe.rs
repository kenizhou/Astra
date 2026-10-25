// SPDX-License-Identifier: GPL-3.0-only
//! What a mail account's setup should ask for: the probe wrapper over
//! the engine's pre-auth verbs, and the three answers the setup screens
//! draw from it.
//!
//! The server is asked **first**, before any OAuth discovery runs: a
//! domain may publish an authorization server for its web sessions and
//! take only a password on IMAP, and a sign-in offered on that evidence
//! mints a token the mailbox refuses. The capability line is what the
//! account will actually be judged by, and the probe reads it with no
//! credential presented and no failed sign-in recorded.

use astra_autodetect::DetectedMailSettings;
use provider_imap::{probe_imap_auth, ImapSecurity};

use crate::mail_oauth::issuer::mail_oauth_issuer;
use crate::ConnectionSecurity;

/// How long the capability probe may take. A server can complete the
/// handshake and then say nothing, so the wait is bounded or the setup
/// screen hangs on a question the person did not ask.
const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_secs(3);

/// How long the issuer search may take before the decision answers
/// without one. The bound is needed because a candidate can accept a
/// connection and never answer.
const ISSUER_SEARCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(6);

/// What a mail account's setup should ask for. Three answers, because
/// they are three screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImapAuth {
    /// Offer "sign in with your provider" as the primary action.
    SignIn {
        /// The authorization server that will be used, for the
        /// diagnostic log and for the host to show which server it is
        /// about to send the person to.
        issuer: String,
        /// The provider's name for the button, when this build holds a
        /// **static** registration that names one. `None` for a server
        /// discovered from the standards, which tells us no name to
        /// print.
        provider_label: Option<String>,
        /// Whether the server also accepts a password, so the client can
        /// offer "use a password instead" beside the button. `false` on
        /// a server that has switched password authentication off, where
        /// that link would be a dead end.
        password_also_works: bool,
    },
    /// The server takes an OAuth token, but only from an application it
    /// registered in advance, and this build holds no registration for
    /// it.
    RegistrationNeeded {
        /// Whether a password still works. Almost always `true`: a
        /// provider in this position generally issues app-specific
        /// passwords precisely because not every client can do its
        /// OAuth.
        password_also_works: bool,
    },
    /// The server advertises no OAuth mechanism, or did not answer. Ask
    /// for a password, which is what works everywhere.
    Password,
}

/// Where to look for an authorization server, and what mail server to
/// ask: everything the decision needs, collected by the setup path.
#[derive(Debug, Clone)]
pub struct ImapAuthQuery<'a> {
    /// The IMAP server to probe: a host, or `host:port`.
    pub imap_host: String,
    /// How the IMAP connection is secured, so the probe reads the
    /// capability the session would actually see.
    pub imap_security: ConnectionSecurity,
    /// The account's email address, whose domain seeds the issuer
    /// search.
    pub email: String,
    /// The detection result for the same address, when one ran: its
    /// `oauth_issuer` field, already parsed and trust-filtered upstream,
    /// is the first issuer candidate.
    pub detected: Option<&'a DetectedMailSettings>,
}

/// Where a usable authorization server came from: the classification's
/// second input, after the server's own answer.
#[derive(Debug, Clone)]
enum Route {
    /// This build holds a static registration covering the server's
    /// host: the provider's own label for the button and its issuer.
    Table(&'static str, &'static str),
    /// An issuer discovered from the standards that admits dynamic
    /// registration.
    Issuer(String),
    /// No route at all: the provider admits only applications it
    /// registered in advance, and this build holds none.
    None,
}

/// The three answers for the server's own word under `route`: pure
/// classification over the offer's two decision flags, separated from
/// the dial so the shapes stay pinnable offline.
fn classify(oauth: bool, password: bool, route: &Route) -> ImapAuth {
    if !oauth {
        return ImapAuth::Password;
    }
    match route {
        Route::Table(label, issuer) => ImapAuth::SignIn {
            issuer: (*issuer).to_owned(),
            provider_label: Some((*label).to_owned()),
            password_also_works: password,
        },
        Route::Issuer(issuer) => ImapAuth::SignIn {
            issuer: issuer.clone(),
            provider_label: None,
            password_also_works: password,
        },
        Route::None => ImapAuth::RegistrationNeeded {
            password_also_works: password,
        },
    }
}

/// Decides what a mail account's setup should ask for: one TLS
/// connection to the dialled server, its capability read by the engine's
/// probe, then the issuer chain when the server offers an OAuth
/// mechanism.
///
/// `tls` is the trust the probe's connection runs under: the same config
/// the wizard's acceptance walk built, so a server whose certificate was
/// accepted is still asked. The issuer chain runs over the shared setup
/// trust instead: an authorization server publishes its metadata through
/// ordinary web PKI, and an accepted mail-server certificate says
/// nothing about it.
///
/// Never fails. Every unanswered question resolves to
/// [`ImapAuth::Password`], which works everywhere; a setup screen that
/// showed an error here would be blocking the person on a question they
/// did not ask.
pub async fn imap_auth_offer(
    query: &ImapAuthQuery<'_>,
    tls: &engine_tls::TlsClientConfig,
) -> ImapAuth {
    let Some(offer) = probe(query, tls).await else {
        return ImapAuth::Password;
    };
    if !offer.oauth {
        log::info!(
            "imap auth: the server advertises no OAuth mechanism; offering a password (it \
             offers: {})",
            mechanisms(&offer),
        );
        return ImapAuth::Password;
    }
    let host = host_of(&query.imap_host);
    // A registration this build already holds beats discovery: the
    // provider has told us in advance which client we are, and
    // re-registering would be neither possible nor wanted.
    if let Some(provider) =
        astra_oauth::provider_for_host(&astra_oauth::static_mail_providers(), host)
    {
        log::info!(
            "imap auth: using this build's registration for {} at {}",
            provider.label,
            provider.issuer,
        );
        return classify(
            offer.oauth,
            offer.password,
            &Route::Table(provider.label, provider.issuer),
        );
    }
    let route = match issuer_within(query).await {
        Some(metadata) => Route::Issuer(metadata.issuer),
        None => Route::None,
    };
    classify(offer.oauth, offer.password, &route)
}

/// Asks the mail server what it accepts, or `None` when it did not
/// answer within the budget.
///
/// The engine's verb dials the connection, reads the pre-auth capability
/// (the post-upgrade one on a STARTTLS dial), and closes with `LOGOUT`;
/// no credential is presented and nothing is attempted that the server
/// would record as a failed sign-in.
async fn probe(
    query: &ImapAuthQuery<'_>,
    tls: &engine_tls::TlsClientConfig,
) -> Option<provider_imap::AuthOffer> {
    let host = host_of(&query.imap_host);
    let addr = dial_addr(&query.imap_host, query.imap_security);
    let connector = tls.connector();
    let asked = probe_imap_auth(
        &addr,
        host,
        engine_security(query.imap_security),
        &connector,
    );
    let Ok(answered) = tokio::time::timeout(PROBE_BUDGET, asked).await else {
        log::info!(
            "imap auth: {host} did not answer the capability probe within {}s",
            PROBE_BUDGET.as_secs(),
        );
        return None;
    };
    match answered {
        Ok(offer) => {
            log::info!(
                "imap auth: {host} offers [{}] (oauth: {}, password: {})",
                mechanisms(&offer),
                offer.oauth,
                offer.password,
            );
            Some(offer)
        }
        Err(err) => {
            log::info!("imap auth: {host} did not answer the capability probe; {err}");
            None
        }
    }
}

/// [`mail_oauth_issuer`] within the decision's issuer budget. Running
/// out is an issuer not found, which is what a candidate that never
/// answers amounts to.
async fn issuer_within(query: &ImapAuthQuery<'_>) -> Option<astra_oauth::AuthServerMetadata> {
    let domain = email_domain(&query.email);
    let found = tokio::time::timeout(
        ISSUER_SEARCH_BUDGET,
        mail_oauth_issuer(domain, query.detected),
    )
    .await;
    let Ok(found) = found else {
        log::info!(
            "imap auth: no authorization server answered within {}s",
            ISSUER_SEARCH_BUDGET.as_secs(),
        );
        return None;
    };
    found
}

/// The domain part of `email`, or the empty string when there is none.
fn email_domain(email: &str) -> &str {
    email
        .trim()
        .rsplit_once('@')
        .map_or("", |(_, domain)| domain.trim())
}

/// The advertised mechanisms as one log-safe string. Protocol atoms,
/// never a credential.
fn mechanisms(offer: &provider_imap::AuthOffer) -> String {
    if offer.mechanisms.is_empty() {
        "none".to_owned()
    } else {
        offer.mechanisms.join(" ")
    }
}

/// The host part of a `host` or `host:port` server field.
fn host_of(server: &str) -> &str {
    let server = server.trim();
    match server.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => server,
    }
}

/// The `host:port` to dial, applying the standard port for the chosen
/// security when the field carries none: the same defaulting the setup
/// builder does, so a probe and the connect that follows it reach the
/// same server.
fn dial_addr(server: &str, security: ConnectionSecurity) -> String {
    let server = server.trim();
    if host_of(server) == server {
        let port = match security {
            ConnectionSecurity::ImplicitTls => 993,
            ConnectionSecurity::StartTls => 143,
        };
        return format!("{server}:{port}");
    }
    server.to_owned()
}

/// The engine's transport enum for this account's connection security.
const fn engine_security(security: ConnectionSecurity) -> ImapSecurity {
    match security {
        ConnectionSecurity::ImplicitTls => ImapSecurity::ImplicitTls,
        ConnectionSecurity::StartTls => ImapSecurity::StartTls,
    }
}

#[cfg(test)]
#[path = "../mail_oauth_probe_tests.rs"]
mod tests;
