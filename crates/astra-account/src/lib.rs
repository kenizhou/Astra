// SPDX-License-Identifier: GPL-3.0-only
//! `astra-account`; load an account's connection config and shape the IMAP connections
//! it drives.
//!
//! It bridges the engine's IMAP adapter (`provider-imap`) and the host: a host reads a
//! TOML config (endpoints plus credentials) out of its OS secure store and this crate
//! turns it into the engine's `ImapConfig` dialled with the account's credential source.
//! The config carries secrets, so it stays out of logs (see [`Secret`]) and out of
//! version control: a real host uses the OS keychain. The account-setup builder that
//! produces this TOML from a host's setup UI lives in [`build_config_toml`].

mod config;
mod credentials;
mod setup;
mod tls;

pub use config::{
    load, load_str, AccountConfig, AccountShape, ConfigError, ConnectionSecurity, ImapAccount,
    Secret, SmtpAccount,
};
pub use credentials::imap_credential_source;
pub use setup::{
    build_config_toml, imap_default_port, smtp_default_port, AccountSetup, SetupCredential,
};
pub use tls::{account_tls, setup_trust_policy, CertificateException};

/// An error shaping an account's connection config.
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    /// The account has no mailbox: there is no IMAP endpoint to connect to.
    #[error("no imap endpoint configured")]
    NoImap,
    /// The account names no credential a connect can present: it stores no password.
    #[error("no usable credential: {0}")]
    MissingCredential(&'static str),
}
