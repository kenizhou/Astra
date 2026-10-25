// SPDX-License-Identifier: GPL-3.0-only
//! The stored-document serialisation of [`AccountConfig`]: the
//! `to_toml` half of the config module, split out so that file stays
//! under the line cap.
//!
//! Manual construction throughout, so [`Secret`](super::Secret) stays
//! deliberately non-serialisable outside this explicit storage boundary.

use super::{AccountConfig, ConfigError, ConnectionSecurity};

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

impl AccountConfig {
    /// Serialises the account to the secure-store TOML shape, including its
    /// redacted-in-memory credentials. Manual construction keeps [`Secret`](super::Secret)
    /// deliberately non-serialisable, so a caller cannot accidentally serialise one outside
    /// this explicit storage boundary.
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
            // The grant path stores no password: a grant-carrying account
            // has none to store, and the load folds a stale one away, so
            // the storage boundary keeps the rule even for a hand-built
            // config.
            if account.oauth.is_none() {
                if let Some(password) = &account.password {
                    imap.insert("password".into(), password.expose().to_owned().into());
                }
            }
            if let Some(grant) = &account.oauth {
                imap.insert("oauth".into(), grant.to_table().into());
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
        if let Some(google) = &self.google {
            let mut section = toml::Table::new();
            section.insert("email".into(), google.email.clone().into());
            section.insert("client_id".into(), google.client_id.clone().into());
            // The non-confidential Desktop secret, persisted only when the
            // account carries one, so a secretless account's stored form
            // stays free of the key.
            if let Some(secret) = &google.client_secret {
                section.insert("client_secret".into(), secret.expose().to_owned().into());
            }
            section.insert("redirect_uri".into(), google.redirect_uri.clone().into());
            section.insert(
                "scopes".into(),
                toml::Value::Array(google.scopes.iter().cloned().map(Into::into).collect()),
            );
            section.insert(
                "refresh_token".into(),
                google.refresh_token.expose().to_owned().into(),
            );
            if let Some(granted) = &google.granted_scopes {
                section.insert(
                    "granted_scopes".into(),
                    toml::Value::Array(granted.iter().cloned().map(Into::into).collect()),
                );
            }
            root.insert("google".into(), section.into());
        }
        if let Some(jmap) = &self.jmap {
            root.insert("jmap".into(), jmap.to_table().into());
        }
        if let Some(crypto) = &self.crypto {
            root.insert(
                "crypto".into(),
                toml::Table::from_iter([("method".into(), crypto.method.clone().into())]).into(),
            );
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
}
