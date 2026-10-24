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
mod crypto;
mod dav;
mod google;
mod graph;
mod imap;
mod jmap;
mod log_handle;
mod oauth_accounts;
mod pass_folders;
mod reconnect;
mod setup;
mod setup_choices;
mod throttle;
mod tls;
mod watch;

pub use autodetect::{recommend, MissReason, OauthRoutes, ServerSummary, SetupRecommendation};
pub use config::{
    load, load_str, AccountConfig, AccountShape, ConfigError, ConnectionSecurity, ImapAccount,
    Secret, SmtpAccount,
};
pub use credentials::imap_credential_source;
pub use crypto::CryptoAccount;
pub use dav::{connect_dav_providers, DavAccount, DavProviders};
pub use google::{
    connect_google_calendar_providers, connect_google_folder, connect_google_mail_providers,
    dial_google_account, fetch_google_address, fetch_google_address_at, google_token_source,
    requested_scopes, withheld, GoogleProviders,
};
pub use graph::{
    connect_graph_calendar_providers, connect_graph_folder, connect_graph_mail_providers,
    dial_microsoft_account, fetch_primary_address, fetch_primary_address_at, CredentialOrigin,
    GraphTokenSource, MicrosoftProviders, TokenSink,
};
pub use imap::{connect_imap_mailbox, connect_mail_providers, ImapConnections};
pub use jmap::{
    begin_jmap_login, begin_jmap_reauth, build_jmap_config_toml, complete_jmap_login,
    complete_jmap_reauth, dial_jmap_account, dial_jmap_oauth_account, jmap_base_url,
    jmap_signin_offered, jmap_token_source, JmapAccount, JmapLoginStart, JmapOAuthGrant, JmapSetup,
    RefreshingJmapProvider,
};
pub use log_handle::account_log_handle;
pub use oauth_accounts::{GoogleAccount, GraphAccount};
pub use pass_folders::pass_syncs;
pub use reconnect::{ReconnectingImapProvider, Redial};
pub use setup::{
    build_config_toml, imap_default_port, smtp_default_port, AccountSetup, SetupCredential,
};
pub use setup_choices::{Capability, SetupChoice};
pub use tls::{account_tls, setup_trust_policy, CertificateException};
pub use watch::{open_watch, Watch, WatchEvent, WatchFace};

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
    /// A Microsoft Graph call failed: a token refresh, or a Graph endpoint
    /// the account wiring reads. A refusal the account cannot recover from
    /// arrives as [`Self::SigninRejected`] instead, so a caller never
    /// mistakes a dead grant for an outage.
    #[error("graph: {0}")]
    Graph(String),
    /// The Graph calendar leg was refused for lack of the calendar scope
    /// (a scope-denied `403` on the calendar-list probe): the account's
    /// grant predates `Calendars.ReadWrite`, so the calendar arrives only
    /// after a fresh sign-in. Apart from [`Self::Graph`] because it is the
    /// one Graph failure a caller answers with a re-consent prompt rather
    /// than an outage badge, and because it costs only the calendar leg:
    /// the account's mail stands.
    #[error("calendar access denied: {0}")]
    CalendarAccessDenied(String),
    /// A Google call failed: a token source build, or a Google endpoint
    /// the account wiring reads (the calendar-list probe). The Google
    /// parallel of [`Self::Graph`]; a refusal the account cannot recover
    /// from arrives as [`Self::SigninRejected`] instead, so a caller
    /// never mistakes a dead grant for an outage. There is no Google
    /// calendar-re-auth case: Google asks for the calendar scope at
    /// connect time, so a refused calendar is an error and never a
    /// banner.
    #[error("google: {0}")]
    Google(String),
    /// A JMAP call failed: the session discovery, the connect, or a
    /// transport failure while building or driving a JMAP account's
    /// provider. A refusal the account cannot recover from arrives as
    /// [`Self::SigninRejected`] instead, so a caller never mistakes a
    /// dead password for an outage.
    #[error("jmap: {0}")]
    Jmap(String),
    /// The IMAP connection or login failed.
    #[error("imap: {0}")]
    Imap(#[from] provider_imap::ImapError),
    /// A standing watch could not be opened or was lost: the engine's push
    /// channel failed outside the refuses it maps to [`Self::Poll`]
    /// (no `IDLE`, no headroom, no event source), so the caller reads the
    /// failure's own class and reconnects or parks on its cadence.
    #[error("watch: {0}")]
    Watch(#[from] engine_provider::ProviderError),
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
    /// answered with `[AUTHENTICATIONFAILED]`, or a JMAP connect's `401`). Distinct from every other variant
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

    /// The verdict for a JMAP connect, which presents the credential on
    /// **every** attempt (the session discovery itself authenticates, so
    /// there is no first-then-folders sequence to corroborate against): an
    /// authentication-class refusal (a `401` to a password or an API
    /// token) becomes [`Self::SigninRejected`]; anything else keeps
    /// [`Self::Jmap`].
    pub(crate) fn from_jmap_connect(err: &provider_jmap::JmapError) -> Self {
        if err.failure_class() == engine_api::FailureClass::Authentication {
            Self::SigninRejected(err.to_string())
        } else {
            Self::Jmap(err.to_string())
        }
    }
}
