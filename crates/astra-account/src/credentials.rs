// SPDX-License-Identifier: GPL-3.0-only
//! What an IMAP account presents to its servers: the credential source its
//! connections are dialled with.
//!
//! The engine asks a [`CredentialSource`] once per IMAP connection the account dials
//! and once per SMTP submission, and once more after a refusal. A password account
//! answers with the same [`Credentials`] every time, which is what the engine's own
//! `Credentials` implementation of the trait does; this module hands the account's
//! stored password to it as that source.

use std::sync::Arc;

use provider_imap::CredentialSource;

use crate::{AccountConfig, AccountError};

/// The credential source for `account`'s IMAP and SMTP connections: its stored
/// password, which answers every dial with the same credentials.
///
/// The engine's [`ImapConfig`](provider_imap::ImapConfig) takes the source rather than
/// a fixed credential, because the account's connections are dialled for as long as the
/// host runs and each one asks anew.
///
/// # Errors
///
/// Returns [`AccountError::MissingCredential`] when the account stores no password,
/// which no config this crate builds does and a hand-edited one can.
pub fn imap_credential_source(
    account: &AccountConfig,
) -> Result<Arc<dyn CredentialSource>, AccountError> {
    let password = account
        .imap_password_credentials()
        .ok_or(AccountError::MissingCredential(
            "the account stores no password",
        ))?;
    Ok(Arc::new(password))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(extra: &str) -> AccountConfig {
        crate::load_str(&format!(
            "[imap]\naddr=\"h:993\"\nserver_name=\"h\"\nusername=\"u\"\n{extra}",
        ))
        .expect("valid config")
    }

    #[test]
    fn a_password_account_is_its_own_credential_source() {
        let source =
            imap_credential_source(&account("password = \"hunter2\"\n")).expect("a source");
        // The source holds the secret redacted, so logging it is safe.
        let dump = format!("{source:?}");
        assert!(dump.contains("username"));
        assert!(!dump.contains("hunter2"));
    }

    #[test]
    fn an_account_without_a_password_is_refused_at_the_source() {
        assert!(matches!(
            imap_credential_source(&account("")),
            Err(AccountError::MissingCredential(_))
        ));
    }
}
