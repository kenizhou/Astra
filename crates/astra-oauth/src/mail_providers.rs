// SPDX-License-Identifier: GPL-3.0-only
//! Mail providers this build carries a **pre-registered** OAuth client
//! for.
//!
//! Most servers that offer OAuth on IMAP need no entry here: they
//! publish `registration_endpoint` and every install mints its own
//! client id from the standards
//! ([`register_client`](crate::register_client)). A few do not. They
//! require an application to be registered with them by hand, usually
//! behind a review, and then a client either holds a registration or
//! cannot use their sign-in at all.
//!
//! The table below is **empty**, and that is a supported build rather
//! than a gap: absent is exactly what a build without a registration is
//! ([`crate::credentials`]), and it degrades the same way. A server that
//! advertises OAuth this build cannot use is reported as such, so a
//! client can say what is actually true (this provider only admits
//! applications it has registered in advance) and offer the password
//! route beside it, rather than showing a sign-in button that dead-ends
//! at the provider.
//!
//! # Adding one
//!
//! Add an entry with the provider's documented endpoints and the host
//! suffixes its mailboxes live under, read its `client_id` from an
//! injected variable, and nothing else changes: the setup path already
//! prefers a discovered registration over a static one, and hides a
//! route whose id is absent.
//!
//! Two things to get right, both of which the shape here forces. The
//! `issuer` must be what the provider's own metadata claims, because a
//! mismatch is refused (RFC 8414 section 3.3) rather than warned about;
//! and a provider that issues a client **secret** is still a public
//! client here, so PKCE has to be what protects the exchange. A provider
//! that will not issue a public client is one to raise with them, not to
//! work around by embedding a confidential secret in a shipped binary.

/// A provider whose OAuth client must be registered up front, and this
/// build's registration for it.
#[derive(Debug, Clone)]
pub struct StaticMailProvider {
    /// The provider's name, as a client shows it on the sign-in button.
    /// User-facing copy.
    pub label: &'static str,
    /// Host suffixes whose mail servers this entry covers, lowercase,
    /// matched on a label boundary (so `mail.yahoo.example` matches
    /// `yahoo.example` and `notyahoo.example` does not).
    pub hosts: &'static [&'static str],
    /// The authorization server's issuer identifier, as its own metadata
    /// claims it.
    pub issuer: &'static str,
    /// The authorization endpoint, from the provider's documentation.
    pub authorize_endpoint: &'static str,
    /// The token endpoint, from the provider's documentation.
    pub token_endpoint: &'static str,
    /// The scopes to request: the provider's own names for mail access,
    /// plus whatever it wants for a refresh token.
    pub scopes: &'static [&'static str],
    /// The registered client id, injected at build time. Not a secret.
    pub client_id: String,
    /// A client secret, where the provider's token endpoint requires one
    /// even of a public client. Injected, never confidential, and PKCE
    /// remains the real protection.
    pub client_secret: Option<String>,
}

/// The static registrations this build carries.
///
/// Empty today. See the module docs: an empty table is a working client,
/// not a missing feature, and the setup path is written so that filling
/// it in changes nothing else.
#[must_use]
pub fn static_mail_providers() -> Vec<StaticMailProvider> {
    Vec::new()
}

/// The entry covering the mail server at `host`, if this build carries
/// one.
///
/// Matched on the **server** host rather than the email domain, because
/// that is what identifies the provider: a custom domain hosted by
/// another provider has that provider's mail server and an address that
/// says nothing.
#[must_use]
pub fn provider_for_host(
    providers: &[StaticMailProvider],
    host: &str,
) -> Option<StaticMailProvider> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    providers
        .iter()
        .find(|provider| {
            provider
                .hosts
                .iter()
                .any(|suffix| covers(&host, &suffix.to_ascii_lowercase()))
        })
        .cloned()
}

/// Whether `host` is `suffix` or a subdomain of it.
///
/// Label-boundary matching, not a substring test: `notyahoo.example`
/// ends with `yahoo.example` as text and is a different organisation, and
/// sending someone's mail credentials to the wrong provider's sign-in
/// page is the failure this guards against.
fn covers(host: &str, suffix: &str) -> bool {
    host == suffix
        || host
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

#[cfg(test)]
#[path = "mail_providers_tests.rs"]
mod mail_providers_tests;
