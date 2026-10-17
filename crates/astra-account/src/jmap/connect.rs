// SPDX-License-Identifier: GPL-3.0-only
//! The JMAP account dial: the one function every path that brings a JMAP
//! account up calls, connecting **one** provider for the whole account.
//!
//! One provider, because JMAP's mail scope is account-wide
//! (`JmapType { account, Email }`) and each message carries its
//! `mailboxIds` membership: every folder syncs through the same
//! connection, and unlike IMAP or Graph there are no per-folder providers
//! to bind. A folder opened before the first pass reconnects this same
//! account-wide provider rather than minting a second one.
//!
//! The dial runs under the account's shared TLS policy and the account's
//! one throttle gate, so a JMAP connection joins its account's ceiling
//! exactly the IMAP, Graph, and DAV legs do.

use engine_tls::TlsClientConfig;
use provider_jmap::{JmapConfig, JmapProvider};

use crate::{throttle::account_retry, AccountConfig, AccountError};

/// Connects the one JMAP provider a JMAP account syncs through: the whole
/// account's mailboxes and every folder's email in one account-wide
/// scope, over the account's shared TLS policy and throttle gate.
///
/// The session the provider snapshots decides what the account has: the
/// capabilities it advertises (mail, calendar, contacts) are read from the
/// server's own session resource, never guessed from the config.
///
/// # Errors
///
/// Returns [`AccountError::MissingCredential`] when the section stores no
/// secret, [`AccountError::SigninRejected`] when the server refuses the
/// account's credential (a `401` the connect presents on every attempt),
/// and [`AccountError::Jmap`] for every other connect failure: an
/// unreachable server, a bad base URL, or a malformed session.
pub async fn dial_jmap_account(
    account: &AccountConfig,
    tls: &TlsClientConfig,
) -> Result<JmapProvider, AccountError> {
    let section = account
        .jmap
        .as_ref()
        .ok_or_else(|| AccountError::Jmap("the account stores no [jmap] section".to_owned()))?;
    let credentials = section
        .credentials()
        .ok_or(AccountError::MissingCredential("jmap"))?;
    // The gate is keyed on the account id the caller's whole dial already
    // uses, so every provider this account connects shares one ceiling
    // against a server that counts requests per account.
    let id = account
        .account_id()
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    let config = JmapConfig::new(section.base_url.clone(), credentials)
        .with_tls(tls.clone())
        .with_retry(account_retry(&id));
    let provider = JmapProvider::connect(config)
        .await
        .map_err(|err| AccountError::from_jmap_connect(&err))?;
    Ok(provider)
}

#[cfg(test)]
#[path = "../jmap_connect_tests.rs"]
mod tests;
