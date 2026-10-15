// SPDX-License-Identifier: GPL-3.0-only
//! One account's connection config: the IMAP/SMTP endpoints and credentials a host
//! reads from its OS secure store; plus loading it from TOML.
//!
//! The config carries secrets, so it stays out of logs (see [`Secret`]) and out of
//! version control: a real host uses the OS keychain. The account-setup builder that
//! produces this TOML from a host's setup UI lives in [`crate::build_config_toml`].

use std::{fmt, path::Path, sync::Arc};

use astra_oauth::OAuthProviderConfig;
use engine_api::AccountId;
use provider_imap::{CredentialSource, Credentials, ImapConfig};
use serde::{Deserialize, Serialize};

use crate::{dav::DavAccount, AccountError, CertificateException};

/// A secret string (password/token) that redacts itself in `Debug`, so a config
/// holding it can still derive `Debug` without leaking the secret into logs.
#[derive(Clone, Deserialize)]
pub struct Secret(String);

impl Secret {
    /// Wraps a raw secret: so a caller outside this crate (the host, building a
    /// credential from a freshly typed password) can construct one.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The underlying secret. Call only at the point of use (building a connection),
    /// never to log or display it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// How a mail connection is secured: TLS from the first byte (implicit TLS, the standard
/// secure ports 993/465), or a cleartext connection upgraded in place with `STARTTLS`
/// (ports 143/587). Mirrors the engine's `ImapSecurity` (`provider-imap`), which is what
/// a config built from this ultimately selects. Defaults to implicit TLS, so an account
/// TOML that names no `security` connects over the standard secure port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionSecurity {
    /// Implicit TLS: the socket is TLS-wrapped before the greeting (ports 993/465).
    #[default]
    ImplicitTls,
    /// STARTTLS: connect in the clear, negotiate the TLS upgrade, then authenticate
    /// (ports 143/587). Credentials never cross the wire before the upgrade.
    StartTls,
}

/// One account's connection config: an IMAP mailbox with optional SMTP
/// submission, the DAV endpoints of its calendar and contacts legs, and
/// the Microsoft section of an account that signs in through Microsoft
/// rather than dialling servers itself. Every section is optional, so a
/// stored document carries what its account's kind needs.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    /// The IMAP mail endpoint. A stored account always names one; a config that does not
    /// is refused when it is loaded.
    #[serde(default)]
    pub imap: Option<ImapAccount>,
    /// The SMTP submission endpoint, if mail-send is configured.
    #[serde(default)]
    pub smtp: Option<SmtpAccount>,
    /// The DAV endpoints, if the account carries calendar and contact legs:
    /// the origins both providers dial, with the credentials they present.
    /// Absent for every mail-only account, which is what every stored
    /// account was before this section existed.
    #[serde(default)]
    pub dav: Option<DavAccount>,
    /// The Microsoft account section, when the account signs in through
    /// Microsoft rather than dialling servers itself: the app registration
    /// its tokens mint through, the refresh token those mints start from,
    /// and the scope set the server last granted. Absent for every account
    /// stored before this section existed.
    #[serde(default)]
    pub graph: Option<GraphAccount>,
    /// Server certificates this account's owner accepted although they did not verify.
    /// Empty for every account that never met one, which is nearly all of them.
    #[serde(default, rename = "certificate_exception")]
    pub certificate_exceptions: Vec<CertificateException>,
    /// The account's pinned id: the key the document's root carries beside its endpoint
    /// sections, read by the loader from there.
    #[serde(skip)]
    pub shape: AccountShape,
}

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

/// An IMAP endpoint: the `host:port` to dial, the TLS server name, and credentials.
#[derive(Debug, Clone, Deserialize)]
pub struct ImapAccount {
    /// The dial address, `host:port` (e.g. `imap.example.net:993`).
    pub addr: String,
    /// The TLS server name for SNI/verification (e.g. `imap.example.net`).
    pub server_name: String,
    /// The login username (the full email address).
    pub username: String,
    /// The login password (or app-specific password).
    #[serde(default)]
    pub password: Option<Secret>,
    /// How the IMAP connection is secured; defaults to implicit TLS (port 993).
    #[serde(default)]
    pub security: ConnectionSecurity,
}

/// An SMTP submission endpoint over implicit TLS or STARTTLS.
#[derive(Debug, Clone, Deserialize)]
pub struct SmtpAccount {
    /// The dial address, `host:port` (e.g. `smtp.example.net:465`).
    pub addr: String,
    /// The TLS server name for SNI/verification (e.g. `smtp.example.net`).
    pub server_name: String,
    /// How the SMTP submission connection is secured; defaults to implicit TLS (port 465).
    #[serde(default)]
    pub security: ConnectionSecurity,
}

/// The shared keys at the root of a stored account document, beside its endpoint
/// sections.
///
/// Every key is optional and written only when set, so a document stored before a key
/// existed reads unchanged and serialises back byte for byte.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AccountShape {
    /// The account's id, once it has been pinned. Absent, the id is derived from the
    /// account's own settings; stored, it survives an edit of the settings it was
    /// derived from.
    #[serde(default)]
    pub id: Option<AccountId>,
}

impl AccountShape {
    /// Reads the shared keys from an account's stored TOML; the endpoint sections are
    /// left to the loader.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Parse`] if one of these keys holds something it cannot.
    pub fn read(text: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(text)?)
    }

    /// Adds the keys that are set to an account document's root table. Nothing is added
    /// for an account that has none, which is what keeps an older document byte for
    /// byte.
    pub(crate) fn write_into(&self, root: &mut toml::Table) {
        if let Some(id) = &self.id {
            root.insert("id".into(), id.as_str().to_owned().into());
        }
    }
}

impl AccountConfig {
    /// Serialises the account to the secure-store TOML shape, including its
    /// redacted-in-memory credentials. Manual construction keeps [`Secret`] deliberately
    /// non-serialisable, so a caller cannot accidentally serialise one outside this
    /// explicit storage boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Serialize`] if TOML encoding fails.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        let mut root = toml::Table::new();
        self.shape.write_into(&mut root);
        if let Some(account) = &self.imap {
            let mut imap = server_table(&account.addr, &account.server_name, account.security);
            imap.insert("username".into(), account.username.clone().into());
            if let Some(password) = &account.password {
                imap.insert("password".into(), password.expose().to_owned().into());
            }
            root.insert("imap".into(), imap.into());
        }
        if let Some(smtp) = &self.smtp {
            root.insert(
                "smtp".into(),
                server_table(&smtp.addr, &smtp.server_name, smtp.security).into(),
            );
        }
        if let Some(dav) = &self.dav {
            let mut section = toml::Table::new();
            if let Some(caldav_url) = &dav.caldav_url {
                section.insert("caldav_url".into(), caldav_url.to_owned().into());
            }
            if let Some(carddav_url) = &dav.carddav_url {
                section.insert("carddav_url".into(), carddav_url.to_owned().into());
            }
            section.insert("username".into(), dav.username.clone().into());
            if let Some(password) = &dav.password {
                section.insert("password".into(), password.expose().to_owned().into());
            }
            root.insert("dav".into(), section.into());
        }
        if let Some(graph) = &self.graph {
            let mut section = toml::Table::new();
            section.insert("email".into(), graph.email.clone().into());
            section.insert("client_id".into(), graph.client_id.clone().into());
            section.insert("tenant".into(), graph.tenant.clone().into());
            section.insert("redirect_uri".into(), graph.redirect_uri.clone().into());
            section.insert(
                "scopes".into(),
                toml::Value::Array(graph.scopes.iter().cloned().map(Into::into).collect()),
            );
            section.insert(
                "refresh_token".into(),
                graph.refresh_token.expose().to_owned().into(),
            );
            if let Some(granted) = &graph.granted_scopes {
                section.insert(
                    "granted_scopes".into(),
                    toml::Value::Array(granted.iter().cloned().map(Into::into).collect()),
                );
            }
            root.insert("graph".into(), section.into());
        }
        if !self.certificate_exceptions.is_empty() {
            let exceptions: Vec<toml::Value> = self
                .certificate_exceptions
                .iter()
                .map(|exception| exception.to_table().into())
                .collect();
            root.insert("certificate_exception".into(), exceptions.into());
        }
        Ok(toml::to_string(&root)?)
    }

    /// The accepted certificates in the engine's form, dropping any whose stored
    /// fingerprint cannot be read: an exception nobody can read is dropped rather than
    /// taking the account down with it.
    #[must_use]
    pub fn tls_exceptions(&self) -> Vec<engine_tls::CertificateException> {
        self.certificate_exceptions
            .iter()
            .filter_map(CertificateException::to_engine)
            .collect()
    }

    /// The engine credentials for the account's stored password, or `None` when it
    /// stores none.
    #[must_use]
    pub fn imap_password_credentials(&self) -> Option<Credentials> {
        let imap = self.imap.as_ref()?;
        imap.password
            .as_ref()
            .map(|password| Credentials::password(&imap.username, password.expose()))
    }

    /// Builds the engine [`ImapConfig`] for this account, asking `credentials` for what
    /// to present on every connection it dials and every submission, and wiring SMTP
    /// submission when configured.
    ///
    /// A source rather than a credential because every connection the account dials
    /// asks it anew; a password account passes its
    /// [`imap_password_credentials`](Self::imap_password_credentials), which is its own
    /// source (see [`crate::imap_credential_source`]).
    ///
    /// # Errors
    ///
    /// Returns [`AccountError::NoImap`] for an account without a mailbox.
    pub fn imap_config(
        &self,
        credentials: Arc<dyn CredentialSource>,
    ) -> Result<ImapConfig, AccountError> {
        let imap = self.imap.as_ref().ok_or(AccountError::NoImap)?;
        let mut config = ImapConfig::from_credential_source(
            imap.addr.clone(),
            imap.server_name.clone(),
            credentials,
        );
        if imap.security == ConnectionSecurity::StartTls {
            config = config.with_starttls();
        }
        Ok(match &self.smtp {
            Some(smtp) => match smtp.security {
                ConnectionSecurity::ImplicitTls => {
                    config.with_smtp_tls(smtp.addr.clone(), smtp.server_name.clone())
                }
                ConnectionSecurity::StartTls => {
                    config.with_smtp_starttls(smtp.addr.clone(), smtp.server_name.clone())
                }
            },
            None => config,
        })
    }

    /// This account's id: the one pinned in its stored config when there is one, and
    /// otherwise the one [`derived_account_id`](Self::derived_account_id) derives. A
    /// pinned id is what lets the settings it was derived from be edited without the
    /// account becoming another one.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Incomplete`] when no id is pinned and the login it would
    /// be derived from is blank, and [`ConfigError::NoEndpoint`] for an account without
    /// a mailbox.
    pub fn account_id(&self) -> Result<AccountId, ConfigError> {
        match &self.shape.id {
            Some(id) => Ok(id.clone()),
            None => self.derived_account_id(),
        }
    }

    /// Derives this account's stable [`AccountId`] from its IMAP login: the **username
    /// and host together**, both **lowercased**. The host disambiguates the same
    /// username on different servers, and lowercasing makes the id stable across case
    /// drift in the typed username, since neither IMAP usernames nor hostnames are
    /// case-sensitive in practice. Hostnames never contain `@`, so `username@host`
    /// splits back unambiguously; distinct `(username, host)` pairs never collide. This
    /// id scopes everything in the shared engine store, so it must be the same for "the
    /// same mailbox" across launches.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Incomplete`] when the login is blank (an account whose
    /// username is empty has no identity to derive), and [`ConfigError::NoEndpoint`] for
    /// an account without a mailbox.
    pub fn derived_account_id(&self) -> Result<AccountId, ConfigError> {
        if let Some(imap) = self.imap.as_ref() {
            let username = imap.username.trim().to_lowercase();
            if username.is_empty() {
                return Err(ConfigError::Incomplete("username"));
            }
            let host = imap.server_name.trim().to_lowercase();
            return AccountId::try_from(format!("{username}@{host}").as_str())
                .map_err(|err| ConfigError::Identity(err.to_string()));
        }
        // A graph-only account has no server of its own: the id keys the
        // grant's mailbox by its address under the one protocol it dials.
        let graph = self.graph.as_ref().ok_or(ConfigError::NoEndpoint)?;
        let email = graph.email.trim().to_lowercase();
        if email.is_empty() {
            return Err(ConfigError::Incomplete("email"));
        }
        AccountId::try_from(format!("{email}@graph").as_str())
            .map_err(|err| ConfigError::Identity(err.to_string()))
    }
}

/// The shared keys of a server section: dial address, TLS server name, and the
/// `security` key only when it is not the default.
fn server_table(addr: &str, server_name: &str, security: ConnectionSecurity) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("addr".into(), addr.to_owned().into());
    table.insert("server_name".into(), server_name.to_owned().into());
    if security == ConnectionSecurity::StartTls {
        table.insert("security".into(), "start_tls".into());
    }
    table
}

/// Loads an [`AccountConfig`] from a TOML file at `path`.
///
/// # Errors
///
/// Returns [`ConfigError`] if the file cannot be read or is not valid config.
pub fn load(path: impl AsRef<Path>) -> Result<AccountConfig, ConfigError> {
    let text = std::fs::read_to_string(path)?;
    load_str(&text)
}

/// Parses an [`AccountConfig`] from a TOML string: the in-memory form a host reads
/// from its OS secure store (Keychain / Windows Credential Manager) rather than a
/// plaintext file on disk.
///
/// # Errors
///
/// Returns [`ConfigError`] if the text is not valid config, or names no
/// server at all (no `[imap]` section and no `[graph]` section).
pub fn load_str(text: &str) -> Result<AccountConfig, ConfigError> {
    let mut config: AccountConfig = toml::from_str(text)?;
    if config.imap.is_none() && config.graph.is_none() {
        return Err(ConfigError::NoEndpoint);
    }
    if let Some(dav) = &config.dav {
        dav.validate()?;
    }
    config.shape = AccountShape::read(text)?;
    Ok(config)
}

/// An error loading account config.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The config file could not be read.
    #[error("reading config: {0}")]
    Io(#[from] std::io::Error),
    /// The config file was not valid TOML / the expected shape.
    #[error("parsing config: {0}")]
    Parse(#[from] toml::de::Error),
    /// A required account-setup field was empty.
    #[error("missing required field: {0}")]
    Incomplete(&'static str),
    /// The config names no server: an account needs an `[imap]` section or
    /// a `[graph]` section.
    #[error("the account names no server: it needs an [imap] or [graph] section")]
    NoEndpoint,
    /// The `[dav]` section names no server: a dav section needs a
    /// `caldav_url`, a `carddav_url`, or both.
    #[error("the dav section names no server: it needs a caldav_url or a carddav_url")]
    NoDavOrigin,
    /// The setup fields could not be serialised to TOML.
    #[error("serialising config: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The account's id could not be derived from its settings.
    #[error("deriving the account id: {0}")]
    Identity(String),
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
