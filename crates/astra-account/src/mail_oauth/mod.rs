// SPDX-License-Identifier: GPL-3.0-only
//! The OAuth half of an IMAP/SMTP account: what the setup path asks a
//! dialled server for, where its authorization server is learned from,
//! and the credential source its connections are dialled with.
//!
//! Three pieces, one per design decision of the SP2v spec:
//!
//! - [`imap_auth_offer`]: the probe wrapper. One TLS connection to the
//!   dialled server, its pre-auth capability read by the engine's verb,
//!   and the three answers the setup screens draw (sign in, sign-in
//!   exists but not for this install, password).
//! - [`mail_oauth_issuer`]: the issuer chain. The provider's own
//!   autoconfig first, then the RFC 8414 well-known of the email domain
//!   and its registrable domain, fail-soft on every miss.
//! - [`imap_oauth_credential_source`]: the token-bearing credential
//!   source over the shared [`GraphTokenSource`], asked per dial, renewing
//!   at most once per refusal.
//!
//! Every failure anywhere in the chain is the password field: a setup
//! screen never shows an error over a question the person did not ask,
//! and the password works everywhere.

mod credentials;
mod grant;
mod issuer;
mod probe;

pub use credentials::{imap_oauth_credential_source, imap_oauth_token_source};
pub use grant::MailOauthGrant;
pub use issuer::{mail_oauth_issuer, register_mail_client};
pub use probe::{imap_auth_offer, ImapAuth, ImapAuthQuery};
