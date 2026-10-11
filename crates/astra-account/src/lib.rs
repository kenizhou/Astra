// SPDX-License-Identifier: GPL-3.0-only
//! `astra-account`; load an account's connection config and shape the IMAP connections
//! it drives.
//!
//! It bridges the engine's IMAP adapter (`provider-imap`) and the host: a host reads a
//! TOML config (endpoints plus credentials) out of its OS secure store and this crate
//! turns it into the engine's `ImapConfig` dialled with the account's credential source.
//! The config carries secrets, so it stays out of logs (see [`Secret`]) and out of
//! version control: a real host uses the OS keychain. The account-setup builder that
//! produces this TOML from a host's setup UI lives in [`build_config_toml`], and the
//! connections a dialled account's folders are bound through in [`ImapConnections`].

mod autodetect;
mod config;
mod credentials;
mod dav;
mod imap;
mod pass_folders;
mod reconnect;
mod setup;
mod setup_choices;
mod tls;

pub use autodetect::{recommend, MissReason, OauthRoutes, ServerSummary, SetupRecommendation};
pub use config::{
    load, load_str, AccountConfig, AccountShape, ConfigError, ConnectionSecurity, ImapAccount,
    Secret, SmtpAccount,
};
pub use credentials::imap_credential_source;
pub use dav::{connect_dav_providers, DavAccount, DavProviders};
pub use imap::{connect_imap_mailbox, connect_mail_providers, ImapConnections};
pub use pass_folders::pass_syncs;
pub use reconnect::{ReconnectingImapProvider, Redial};
pub use setup::{
    build_config_toml, imap_default_port, smtp_default_port, AccountSetup, SetupCredential,
};
pub use setup_choices::{Capability, SetupChoice};
pub use tls::{account_tls, setup_trust_policy, CertificateException};

/// An error shaping an account's connection config or connecting its providers.
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    /// The account has no mailbox: there is no IMAP endpoint to connect to.
    #[error("no imap endpoint configured")]
    NoImap,
    /// The account names no credential a connect can present: it stores no password.
    #[error("no usable credential: {0}")]
    MissingCredential(&'static str),
    /// The mailbox id was not valid.
    #[error("invalid mailbox id: {0}")]
    Mailbox(String),
    /// The CalDAV or CardDAV connection failed: the transport, the discovery,
    /// or the login. Apart from the mail variants because a failed dav leg
    /// leaves an otherwise working account: the caller logs it and keeps the
    /// mail half standing.
    #[error("dav: {0}")]
    Dav(String),
    /// Listing the account's folders failed (the dial needs it to bind them).
    #[error("listing mailboxes: {0}")]
    MailboxList(String),
    /// The IMAP connection or login failed.
    #[error("imap: {0}")]
    Imap(#[from] provider_imap::ImapError),
    /// The server presented a certificate that did not verify, and this is the certificate
    /// it presented.
    ///
    /// Apart from every other connect failure because it is the one a person can settle
    /// without changing anything about the account: they are shown what the server
    /// offered and may accept that one certificate for that one server as a
    /// [`CertificateException`]. Boxed because a certificate is kilobytes while every
    /// other variant here is a string.
    #[error("certificate not verified: {reason}")]
    CertificateRejected {
        /// What the transport said, kept for the diagnostic log.
        reason: String,
        /// The server name asked for, and the certificate it answered with.
        rejected: Box<engine_tls::RejectedCertificate>,
    },
    /// Building the account's shared TLS policy failed.
    #[error("tls: {0}")]
    Tls(#[from] engine_tls::TlsError),
    /// The server refused the account's **stored credential** outright (a password
    /// answered with `[AUTHENTICATIONFAILED]`). Distinct from every other variant
    /// because it is the one failure a retry cannot fix and an outage badge
    /// misdescribes (the server *was* reached) so a caller prompts "sign in again".
    #[error("sign-in rejected: {0}")]
    SigninRejected(String),
}

impl AccountError {
    /// Re-reads a connect failure as a certificate refusal when `tls` recorded one.
    ///
    /// The transport reports a refused certificate as a transport error like any other, so
    /// *which* certificate was refused exists only in the TLS config that refused it. One
    /// config is built per connect, so what it holds belongs to this failure.
    pub(crate) fn over_tls(self, tls: &engine_tls::TlsClientConfig) -> Self {
        match tls.rejected() {
            Some(rejected) => Self::CertificateRejected {
                reason: self.to_string(),
                rejected: Box::new(rejected),
            },
            None => self,
        }
    }

    /// The verdict for the login that **first** presents an IMAP account's password in a dial;
    /// the only one whose refusal can mean the password itself is no good. An
    /// [authentication-class](engine_api::FailureClass::Authentication) refusal becomes
    /// [`Self::SigninRejected`]; anything else keeps [`Self::Imap`].
    ///
    /// A later connection of the same dial deliberately does **not** come through here. The same
    /// password authenticated seconds earlier, so a refusal there is the server contradicting
    /// itself, and prompting for a new sign-in over it is the false prompt; servers do refuse a
    /// valid credential.
    pub(crate) fn from_first_imap_login(err: provider_imap::ImapError) -> Self {
        if err.failure_class() == engine_api::FailureClass::Authentication {
            Self::SigninRejected(err.to_string())
        } else {
            Self::Imap(err)
        }
    }
}
