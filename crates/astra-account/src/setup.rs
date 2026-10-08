// SPDX-License-Identifier: GPL-3.0-only
//! The account-setup builder; turning the fields a host collects in its setup UI into
//! the account-config TOML it stores, the inverse of [`crate::load_str`].
//!
//! Servers are given as a bare host; the standard secure port is assumed and the TLS
//! server name is derived from the host, so users never type ports or a separate "server
//! name". Credentials go straight into the stored TOML; never a plaintext seed file.

use serde::{Deserialize, Serialize};

use crate::{CertificateException, ConfigError, ConnectionSecurity};

/// What the account will authenticate with, from the screen that collected it.
///
/// The two are alternatives rather than a preference: a setup screen offers whichever
/// the server said it accepts, so by the time anything reaches here the choice is
/// already made.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupCredential {
    /// A password or app-specific password, typed into the form.
    Password(String),
    /// A browser sign-in grant. An account built with one carries no stored password;
    /// its credential is presented at connect by the token source the sign-in installs.
    Grant,
}

/// The fields a host collects in its account-setup UI, so it can build a config without
/// a plaintext seed file: the host serialises this with [`build_config_toml`] and stores
/// the result in its OS secure store.
///
/// Servers are given as a bare host (e.g. `imap.example.net`); the standard secure port
/// for the chosen security is assumed and the TLS server name is the host, so users
/// never type ports or a separate "server name". A power user may still pass an
/// explicit `host:port`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSetup {
    /// IMAP mail server: a host (`imap.example.net`) or `host:port`.
    pub imap_host: String,
    /// Login username (the full email address).
    pub username: String,
    /// What the account authenticates with: a typed password, or a completed sign-in.
    pub credential: SetupCredential,
    /// SMTP server (host or `host:port`), if mail-send is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp_host: Option<String>,
    /// CalDAV base URL, if calendar sync is configured. Written into the stored TOML
    /// for the slice that connects calendars, with a bare host gaining `https://`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caldav_base_url: Option<String>,
    /// How the IMAP connection is secured; picks the default port (993 vs 143) and the
    /// engine's connect path. Defaults to implicit TLS.
    pub imap_security: ConnectionSecurity,
    /// How the SMTP submission connection is secured; picks the default port (465 vs
    /// 587). Defaults to implicit TLS.
    pub smtp_security: ConnectionSecurity,
    /// A server certificate somebody accepted for this account after being shown it,
    /// because it did not verify. `None` for every setup that never met one, which is
    /// nearly all of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_certificate: Option<CertificateException>,
}

/// The standard implicit-TLS IMAP port, assumed when a host gives none.
const DEFAULT_IMAP_TLS_PORT: u16 = 993;
/// The standard STARTTLS IMAP port, assumed when a STARTTLS host gives none.
const DEFAULT_IMAP_STARTTLS_PORT: u16 = 143;
/// The standard implicit-TLS SMTP submission port, assumed when a host gives none.
const DEFAULT_SMTP_TLS_PORT: u16 = 465;
/// The standard STARTTLS SMTP submission port, assumed when a STARTTLS host gives none.
const DEFAULT_SMTP_STARTTLS_PORT: u16 = 587;

/// The standard IMAP port for a given connection security.
///
/// Public because the setup forms show it: a manual form starts at the standard port
/// for the security the user picked, and several surfaces each carrying their own copy
/// of 993 would be several places for it to go stale.
#[must_use]
pub const fn imap_default_port(security: ConnectionSecurity) -> u16 {
    match security {
        ConnectionSecurity::ImplicitTls => DEFAULT_IMAP_TLS_PORT,
        ConnectionSecurity::StartTls => DEFAULT_IMAP_STARTTLS_PORT,
    }
}

/// The standard SMTP submission port for a given connection security. Public for the
/// reason [`imap_default_port`] is.
#[must_use]
pub const fn smtp_default_port(security: ConnectionSecurity) -> u16 {
    match security {
        ConnectionSecurity::ImplicitTls => DEFAULT_SMTP_TLS_PORT,
        ConnectionSecurity::StartTls => DEFAULT_SMTP_STARTTLS_PORT,
    }
}

/// Splits a user-entered server into its TLS server name (the host) and dial address
/// (`host:port`), applying `default_port` when the input carries none: so a user can
/// type just `imap.example.net` and never needs to know server ports. An explicit
/// `host:port` is preserved.
fn host_and_addr(input: &str, default_port: u16) -> (String, String) {
    if let Some((host, port)) = input.rsplit_once(':') {
        if !host.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
            return (host.to_owned(), input.to_owned());
        }
    }
    (input.to_owned(), format!("{input}:{default_port}"))
}

/// Writes the `security` key into a server section, but only for STARTTLS; implicit
/// TLS is the config's default (`#[serde(default)]`), so omitting the key keeps a
/// standard account's TOML unchanged and round-trips to the same
/// [`ConnectionSecurity`].
fn insert_security(section: &mut toml::Table, security: ConnectionSecurity) {
    if security == ConnectionSecurity::StartTls {
        section.insert("security".into(), "start_tls".into());
    }
}

/// Ensures a CalDAV base URL carries a scheme, defaulting to `https://` when the user
/// typed a bare host (e.g. `caldav.example.net`): the calendar counterpart of the
/// default-port assumption for IMAP/SMTP, so a user never needs to type `https://`. An
/// explicit scheme (`http://`, `https://`) is preserved.
fn normalize_caldav_base_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    }
}

/// A form field's value, trimmed, when it holds one.
fn given(field: Option<&String>) -> Option<&str> {
    field
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

/// Serialises [`AccountSetup`] into an account-config TOML string: the inverse of
/// [`load_str`](crate::load_str). Servers without an explicit port get the standard
/// secure port and the TLS server name is derived from the host. The `smtp` section is
/// emitted only when supplied; a CalDAV section, when one is given, reuses the IMAP
/// username/password (the common single-credential case). A host stores the result in
/// its OS secure store, then loads it: so credentials never touch a plaintext file.
///
/// # Errors
///
/// Returns [`ConfigError::Incomplete`] if a required field (username, password, the
/// mail server) is empty, or [`ConfigError::Serialize`] on a serialisation error.
pub fn build_config_toml(setup: &AccountSetup) -> Result<String, ConfigError> {
    if setup.imap_host.trim().is_empty() {
        return Err(ConfigError::Incomplete("mail server"));
    }
    if setup.username.trim().is_empty() {
        return Err(ConfigError::Incomplete("username"));
    }
    if matches!(&setup.credential, SetupCredential::Password(password) if password.is_empty()) {
        return Err(ConfigError::Incomplete("password"));
    }

    let username = setup.username.trim();
    let mut root = toml::Table::new();
    let (imap_name, imap_addr) = host_and_addr(
        setup.imap_host.trim(),
        imap_default_port(setup.imap_security),
    );
    let mut imap = toml::Table::new();
    imap.insert("addr".into(), imap_addr.into());
    imap.insert("server_name".into(), imap_name.into());
    imap.insert("username".into(), username.into());
    if let SetupCredential::Password(password) = &setup.credential {
        imap.insert("password".into(), password.clone().into());
    }
    insert_security(&mut imap, setup.imap_security);
    root.insert("imap".into(), imap.into());

    if let Some(host) = given(setup.smtp_host.as_ref()) {
        let (name, addr) = host_and_addr(host, smtp_default_port(setup.smtp_security));
        let mut smtp = toml::Table::new();
        smtp.insert("addr".into(), addr.into());
        smtp.insert("server_name".into(), name.into());
        insert_security(&mut smtp, setup.smtp_security);
        root.insert("smtp".into(), smtp.into());
    }

    if let Some(url) = given(setup.caldav_base_url.as_ref()) {
        let mut caldav = toml::Table::new();
        caldav.insert("base_url".into(), normalize_caldav_base_url(url).into());
        caldav.insert("username".into(), username.into());
        if let SetupCredential::Password(password) = &setup.credential {
            caldav.insert("password".into(), password.clone().into());
        }
        root.insert("caldav".into(), caldav.into());
    }

    if let Some(accepted) = &setup.accepted_certificate {
        root.insert(
            "certificate_exception".into(),
            vec![toml::Value::from(accepted.to_table())].into(),
        );
    }
    Ok(toml::to_string(&root)?)
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
