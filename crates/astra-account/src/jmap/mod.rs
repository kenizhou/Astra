// SPDX-License-Identifier: GPL-3.0-only
//! The JMAP half of an account: the optional `[jmap]` section of a stored
//! config (RFC 8620/8621), the setup builder that writes it, the dial
//! that connects the engine's one provider for the account, and the OAuth
//! sign-in ([`oauth`]) a server that advertises one connects it by.
//!
//! A JMAP account is a base URL and **one** secret: a login password, an
//! app-specific password, or an API token, whichever the server issued.
//! The section stores it as `password` beside the account address, and the
//! engine's transport presents it under whichever scheme the server's own
//! `401` challenge names, so one stored form works against a server that
//! expects Basic or Bearer.
//!
//! A server that advertises discoverable OAuth (RFC 9728) offers a second
//! way in: the browser sign-in [`oauth::begin_jmap_login`] runs, whose
//! grant persists as `[jmap.oauth]` with no long-lived password beside
//! it, and whose access token the [`refreshing`] wrapper keeps live for
//! the whole session.
//!
//! Unlike IMAP or Graph, one provider serves the whole account: JMAP's mail
//! scope is account-wide and each message carries its `mailboxIds`
//! membership, so a single connection syncs every folder and there are no
//! per-folder providers to bind.

mod connect;
mod oauth;
mod refreshing;

pub use connect::{dial_jmap_account, dial_jmap_oauth_account};
pub use oauth::{
    begin_jmap_login, begin_jmap_reauth, complete_jmap_login, complete_jmap_reauth,
    jmap_signin_offered, jmap_token_source, JmapLoginStart,
};
pub use refreshing::RefreshingJmapProvider;

use provider_jmap::Credentials;
use serde::Deserialize;

use crate::{ConfigError, Secret};

/// The scheme tag woven into a JMAP account's id, so a JMAP account never
/// collides with an IMAP or an OAuth account for the same address on the
/// same host: IMAP keys on `address@host`, the OAuth families on
/// `address@graph` and `address@google`, JMAP on `address@jmap:<host>`.
/// The tag cannot appear in a real IMAP `server_name`, so the id spaces
/// stay disjoint.
pub(crate) const JMAP_ID_SCHEME: &str = "jmap";

/// The optional `[jmap]` section of a stored account: the account address,
/// the server base URL, and exactly one secret.
///
/// The secret always reads into [`JmapAccount::password`], whatever key it
/// was stored under: a legacy `token` key (from a document written before
/// the two secret fields were collapsed to one) folds into it at load, and
/// no code path can ever see both. Stored with the email as the username,
/// the secret is the strictly more capable form: the engine presents it as
/// `Basic` or re-frames it as `Bearer` when a server challenges for one,
/// while a bare token has no username to build a Basic header from.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "RawJmapAccount")]
pub struct JmapAccount {
    /// The account's email address: the login username, the account's
    /// identity, and the basis of its id.
    pub email: String,
    /// The JMAP server base URL to connect to (e.g.
    /// `https://mail.example.net`, or `http://127.0.0.1:2080` for the
    /// harness). The session resource, the API URL, and the account ids
    /// are discovered from it.
    pub base_url: String,
    /// The account's one secret: a login password, an app-specific
    /// password, or an API token. A legacy `token` key folds into this
    /// field at load; the section never holds two secrets.
    #[serde(default)]
    pub password: Option<Secret>,
    /// The `[jmap.oauth]` block, when the account was connected by a
    /// browser sign-in rather than a pasted secret. `None` for every
    /// account the password form writes; present, the account stores no
    /// long-lived password at all, and the block's refresh token is its
    /// one credential.
    #[serde(default)]
    pub oauth: Option<JmapOAuthGrant>,
    // A document somehow carrying both a password and a grant: the grant
    // wins (mail-calendar's precedence). The host never writes both, and
    // a grant-carrying account was connected by signing in.
}

/// The wire form of [`JmapAccount`]: the only place the legacy `token` key
/// exists, existing solely to fold it into [`JmapAccount::password`].
#[derive(Deserialize)]
struct RawJmapAccount {
    email: String,
    base_url: String,
    #[serde(default)]
    password: Option<Secret>,
    /// The bearer-only API-token key a document written before the
    /// one-secret collapse carries. Read and folded, never written again.
    #[serde(default)]
    token: Option<Secret>,
    #[serde(default)]
    oauth: Option<JmapOAuthGrant>,
}

impl From<RawJmapAccount> for JmapAccount {
    fn from(raw: RawJmapAccount) -> Self {
        Self {
            email: raw.email,
            base_url: raw.base_url,
            // The fold, and its one precedence rule: a document carrying
            // both keys keeps the `token` value, the one its account was
            // last connected with.
            password: raw.token.or(raw.password),
            oauth: raw.oauth,
        }
    }
}

/// The `[jmap.oauth]` block of a browser-signed-in account: everything
/// needed to mint a fresh access token at launch without repeating
/// discovery. Written by [`complete_jmap_login`](crate::complete_jmap_login)
/// and read by [`jmap_token_source`](crate::jmap_token_source); an OAuth
/// account stores this block and **no** long-lived password.
///
/// The optional fields (`client_secret`, `resource`, `issuer`) carry
/// `#[serde(default)]` so a document written before the field existed
/// still loads: absent means "nothing to send / nothing to compare", which
/// is the pre-existing behaviour rather than a fault.
#[derive(Debug, Clone, Deserialize)]
pub struct JmapOAuthGrant {
    /// The client identifier, from dynamic registration (RFC 7591) at
    /// sign-in. Not a secret. Persisted so a launch never re-registers.
    pub client_id: String,
    /// A client secret, on the rare server that issued one to a client
    /// that asked to be public. Sent on the token exchange and the refresh
    /// when present. **Not** confidential; an installed app cannot keep a
    /// secret; PKCE is the real protection.
    #[serde(default)]
    pub client_secret: Option<Secret>,
    /// The long-lived refresh token. Rotated in place when the server
    /// issues a new one, and re-persisted through the host's
    /// [`TokenSink`](crate::TokenSink): a ratcheting server revokes the
    /// whole grant over a replayed token, so a rotation nobody stored
    /// kills the account at the next launch.
    pub refresh_token: Secret,
    /// The discovered authorization endpoint, kept so a re-consent needs no
    /// re-discovery.
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
    /// The RFC 8707 resource indicator: the canonical URI of the resource
    /// this grant is for, from the server's RFC 9728 metadata. Persisted
    /// because **every refresh must re-send it**: a server that issues
    /// resource-scoped tokens answers `invalid_target` without it, so
    /// dropping it here would break the account at the first refresh
    /// rather than at setup. `None` for a server that publishes no
    /// `resource`.
    #[serde(default)]
    pub resource: Option<String>,
    /// The issuer to check a re-authorisation's `iss` against
    /// (RFC 9207), when the server advertised that it sends one. `None`
    /// for a server that does not: absent means "nothing to compare",
    /// which is the pre-RFC-9207 status quo and not a reason to break a
    /// working account.
    #[serde(default)]
    pub issuer: Option<String>,
}

impl JmapOAuthGrant {
    /// The OAuth client config this grant refreshes through: its own
    /// endpoints, its registered client id, and
    /// [`AuthStyle::Discovered`](astra_oauth::AuthStyle::Discovered),
    /// which sends only the parameters RFC 6749 and RFC 7636 define,
    /// because this server's documentation is unread and vendor
    /// extensions it may reject are not guessed at.
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
            resource: self.resource.clone(),
            expected_issuer: self.issuer.clone(),
            style: astra_oauth::AuthStyle::Discovered,
        }
    }

    /// The grant as the `[jmap.oauth]` table the stored document carries:
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
        if let Some(resource) = &self.resource {
            table.insert("resource".into(), resource.clone().into());
        }
        if let Some(issuer) = &self.issuer {
            table.insert("issuer".into(), issuer.clone().into());
        }
        table
    }
}

impl JmapAccount {
    /// The engine credentials the stored secret presents: HTTP Basic under
    /// the account address, whatever the user pasted. `None` when the
    /// section stores no secret at all.
    ///
    /// The username-bearing form is chosen over a bare bearer token
    /// because it is strictly more capable: the engine presents it as
    /// `Basic` or re-frames it as `Bearer` when the server's own `401`
    /// challenge asks for that, so one stored secret serves a server that
    /// changes its mind about the wire format.
    pub(crate) fn credentials(&self) -> Option<Credentials> {
        self.password
            .as_ref()
            .map(|password| Credentials::basic(self.email.clone(), password.expose().to_owned()))
    }

    /// The section as the table the stored document carries it in: the
    /// inverse of deserialisation, manual so [`Secret`] stays
    /// non-serialisable outside this boundary. Only the one secret is
    /// written, as `password`, and only for an account the password form
    /// connected; a grant block, when present, writes its own sub-table.
    pub(crate) fn to_table(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("email".into(), self.email.clone().into());
        table.insert("base_url".into(), self.base_url.clone().into());
        if let Some(password) = &self.password {
            table.insert("password".into(), password.expose().to_owned().into());
        }
        if let Some(grant) = &self.oauth {
            table.insert("oauth".into(), grant.to_table().into());
        }
        table
    }

    /// The account id a jmap section derives: the lowercased address under
    /// the JMAP scheme tag and the base URL's host. The one derivation
    /// every jmap account's id walks, kept beside the section so the
    /// config loader and the re-authentication's same-mailbox check cannot
    /// drift apart.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Incomplete`] when the address is blank.
    pub(crate) fn derived_account_id(&self) -> Result<engine_api::AccountId, ConfigError> {
        let email = self.email.trim().to_lowercase();
        if email.is_empty() {
            return Err(ConfigError::Incomplete("email"));
        }
        let host = base_host(&self.base_url);
        engine_api::AccountId::try_from(format!("{email}@{JMAP_ID_SCHEME}:{host}").as_str())
            .map_err(|err| ConfigError::Identity(err.to_string()))
    }
}

/// The fields a host's JMAP account-setup form collects, so first-run
/// setup needs no plaintext seed file: the host passes this to
/// [`build_jmap_config_toml`], stores the result in its OS secure store,
/// then loads it.
///
/// **One** secret field, never two. A JMAP server declares its
/// authentication scheme in the `WWW-Authenticate` challenge of its `401`
/// (RFC 8620 §8.2 specifies none of its own) and the engine negotiates
/// from that, so whether the user pasted a login password, an
/// app-specific password, or an API token changes nothing this form needs
/// to ask.
#[derive(Debug, Clone)]
pub struct JmapSetup {
    /// The account's email address (the username and the account id
    /// basis).
    pub email: String,
    /// The JMAP server (host, `host:port`, or full URL). Empty derives
    /// `https://<email-domain>` and discovers the session at
    /// `/.well-known/jmap`.
    pub server_url: Option<String>,
    /// The account's secret: a login password, an app-specific password,
    /// or an API token, whichever the server issued. Stored as
    /// `password` with the email as the username.
    pub password: String,
}

/// Serialises a [`JmapSetup`] into the `[jmap]` account TOML a host stores
/// in its OS secure store: the JMAP counterpart of
/// [`build_config_toml`](crate::build_config_toml).
///
/// The secret is always written as **`password`**, never `token`, whatever
/// the user pasted: paired with the email as the username it is the form
/// the engine can present under either scheme the server challenges for.
///
/// # Errors
///
/// Returns [`ConfigError::Incomplete`] if the email or the secret is
/// empty, or an empty server URL cannot be derived because the email has
/// no domain, or [`ConfigError::Serialize`] on a serialisation error.
pub fn build_jmap_config_toml(setup: &JmapSetup) -> Result<String, ConfigError> {
    let email = setup.email.trim();
    if email.is_empty() {
        return Err(ConfigError::Incomplete("email"));
    }
    let password = setup.password.trim();
    if password.is_empty() {
        return Err(ConfigError::Incomplete("password"));
    }
    let account = JmapAccount {
        email: email.to_owned(),
        base_url: jmap_base_url(email, setup.server_url.as_deref())?,
        password: Some(Secret::new(password.to_owned())),
        oauth: None,
    };
    let mut root = toml::Table::new();
    root.insert("jmap".into(), account.to_table().into());
    Ok(toml::to_string(&root)?)
}

/// Resolves a JMAP account's base URL from what the setup form collected:
/// an explicit server URL, or (when blank) `https://<email-domain>`
/// relying on `/.well-known/jmap` autodiscovery.
///
/// The one derivation every stored JMAP document and every connect walks so a discovered
/// account and a hand-entered one cannot disagree about which server they
/// mean.
///
/// # Errors
///
/// Returns [`ConfigError::Incomplete`] if the server URL is blank and the
/// email has no domain to derive one from.
pub fn jmap_base_url(email: &str, server_url: Option<&str>) -> Result<String, ConfigError> {
    let server_url = server_url.map(str::trim).filter(|url| !url.is_empty());
    if let Some(url) = server_url {
        return Ok(normalize_base_url(url));
    }
    let domain = email
        .trim()
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim())
        .filter(|domain| !domain.is_empty())
        .ok_or(ConfigError::Incomplete("server URL"))?;
    Ok(format!("https://{domain}"))
}

/// Ensures a JMAP base URL carries a scheme, defaulting a bare host to
/// `https://`. An explicit scheme is preserved, with one exception: an
/// explicit `http://` survives only for a loopback host, the form the
/// harness fixture speaks; on any other host it is upgraded to `https://`,
/// because a real account always dials TLS and a person who typed
/// `http://` at a public server meant the server, not the cleartext.
fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return format!("https://{trimmed}");
    };
    if scheme.eq_ignore_ascii_case("http") && !is_loopback(rest) {
        return format!("https://{rest}");
    }
    trimmed.to_owned()
}

/// Whether a URL's `host:port/path` part names this machine: the one host
/// class an explicit `http://` base URL is preserved for.
fn is_loopback(authority: &str) -> bool {
    let host = authority.split('/').next().unwrap_or(authority).trim();
    let host = if let Some(bracketed) = host.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or(bracketed)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
        || host
            .parse::<std::net::Ipv6Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The host portion (`host` or `host:port`, lowercased) of a JMAP base
/// URL, for the account id. Deterministic string surgery (no URL parsing)
/// so the derived id is stable: strip an optional scheme, then take
/// everything up to the first `/`.
pub(crate) fn base_host(base_url: &str) -> String {
    let no_scheme = base_url
        .split_once("://")
        .map_or(base_url, |(_, rest)| rest);
    no_scheme
        .split('/')
        .next()
        .unwrap_or(no_scheme)
        .trim()
        .to_lowercase()
}

#[cfg(test)]
#[path = "../jmap_tests.rs"]
mod tests;
