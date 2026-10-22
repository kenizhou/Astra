// SPDX-License-Identifier: GPL-3.0-only
//! The host's [`CryptoTransportResolver`]: the dial table's answer to "how
//! does this account send crypto mail". The method comes from the stored
//! config's `[crypto]` section; the dial table from its `[imap]`/`[smtp]`
//! sections and the account's trust policy (certificate exceptions
//! included), exactly as the boot dials them. Secrets stay in-process:
//! the resolver's answer never crosses an IPC boundary.

use std::sync::Arc;

use async_trait::async_trait;
use engine_api::AccountId;

use astra_app::{CryptoTransport, CryptoTransportResolver};
use astra_crypto_core::CryptoMethod;
use astra_crypto_store::{ImapDial, SmtpDial, TransportSecurity};

use crate::dial_table::AccountDials;

/// The host's crypto resolver over the dial table.
pub(crate) struct HostCryptoResolver {
    dials: Arc<AccountDials>,
}

impl HostCryptoResolver {
    /// A resolver over the boot's dial table.
    pub(crate) fn new(dials: Arc<AccountDials>) -> Self {
        Self { dials }
    }

    /// The stored config whose derived (or pinned) account id is `account`.
    fn config_for(&self, account: &AccountId) -> Option<astra_account::AccountConfig> {
        for id in self.dials.ids() {
            let Some((config, _connections)) = self.dials.of(&id) else {
                continue;
            };
            match config.account_id() {
                Ok(candidate) if candidate == *account => return Some(config),
                Ok(_) => {}
                Err(err) => {
                    log::warn!(
                        "crypto: a registered account could not derive its id and is skipped: {err}"
                    );
                }
            }
        }
        None
    }
}

/// Map the config layer's security enum onto the transport's (the same two
/// shapes, one enum per crate, so the transport names no account layer).
fn security_of(security: astra_account::ConnectionSecurity) -> TransportSecurity {
    match security {
        astra_account::ConnectionSecurity::ImplicitTls => TransportSecurity::ImplicitTls,
        astra_account::ConnectionSecurity::StartTls => TransportSecurity::StartTls,
    }
}

#[async_trait]
impl CryptoTransportResolver for HostCryptoResolver {
    async fn crypto_method(&self, account: &AccountId) -> CryptoMethod {
        let Some(config) = self.config_for(account) else {
            return CryptoMethod::None;
        };
        config.crypto.as_ref().map_or(CryptoMethod::None, |crypto| {
            CryptoMethod::from_db_str(&crypto.method)
        })
    }

    async fn crypto_transport(&self, account: &AccountId) -> Option<CryptoTransport> {
        let config = self.config_for(account)?;
        let method = config.crypto.as_ref().map_or(CryptoMethod::None, |crypto| {
            CryptoMethod::from_db_str(&crypto.method)
        });
        if method == CryptoMethod::None {
            return None;
        }
        // The typed unavailable answer: a method set on an account without
        // both dial halves (a Graph/Google/JMAP account, or IMAP without
        // SMTP) resolves to None and the send holds, never a silent drop.
        let imap = config.imap.as_ref()?;
        let smtp = config.smtp.as_ref()?;
        let password = imap.password.as_ref()?.expose().to_owned();
        let connector = astra_account::account_tls(&config).ok()?.connector();
        Some(CryptoTransport {
            method,
            account_email: imap.username.clone(),
            smtp: SmtpDial {
                addr: smtp.addr.clone(),
                server_name: smtp.server_name.clone(),
                security: security_of(smtp.security),
                username: imap.username.clone(),
                password: password.clone(),
            },
            imap: ImapDial {
                addr: imap.addr.clone(),
                server_name: imap.server_name.clone(),
                security: security_of(imap.security),
                username: imap.username.clone(),
                password,
            },
            connector,
        })
    }
}

#[cfg(test)]
#[path = "crypto_send_tests.rs"]
mod crypto_send_tests;
