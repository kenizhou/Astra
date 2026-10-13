// SPDX-License-Identifier: GPL-3.0-only
//! The OAuth client registrations a build carries, read from the
//! environment at **compile time**. The single place any Google or
//! Microsoft client id or secret enters the product; nothing else in the
//! tree may hold one.
//!
//! A registration is optional, and its absence is a supported build. A
//! build without Google's registration does not offer Google sign-in at
//! all: the setup wizard drops the route and detection stops recommending
//! it, so a from-source build is a working open-standards mail client
//! (IMAP, JMAP) with the password routes, never a dead button. The same
//! holds for Microsoft. A build given no registration still connects
//! mail; the routes that need one are simply not offered.
//!
//! **A blank value is an absent value.** A CI run without access to the
//! registrations sets the variables to the empty string rather than
//! leaving them unset, and a build carrying `client_id=""` would fail at
//! the provider instead of at the wizard.
//!
//! Google issues a **separate client per client type**, and this product
//! ships the desktop one: `ASTRA_GOOGLE_DESKTOP_CLIENT_ID` with
//! `ASTRA_GOOGLE_DESKTOP_CLIENT_SECRET` (required by Google's token
//! endpoint even under PKCE, non-confidential) and a loopback redirect
//! the host supplies per flow. Microsoft registers one app for the
//! platform, so `ASTRA_MS_CLIENT_ID` is the whole of its registration.
//!
//! None of these values is confidential. The client ids are public by
//! construction, and the Desktop client secret is one Google's own
//! installed-app guidance says "is obviously not treated as a secret":
//! an installed binary cannot keep one, and it grants nothing without a
//! fresh PKCE verifier and the user's consent
//! (<https://developers.google.com/identity/protocols/oauth2#installed>).
//! They are injected rather than committed so that the tree can be
//! published without carrying one project's registrations, not because
//! embedding them endangers anything.

/// Google's OAuth client registration for this build, or `None` when it
/// carries none.
///
/// The `client_id` and `client_secret` this build was given for Google's
/// Desktop client type, whose redirect is a loopback port the host binds
/// per flow (so no redirect travels with the registration).
#[derive(Debug, Clone)]
pub struct GoogleClient {
    /// The client id, of the form
    /// `<project-number>-<random>.apps.googleusercontent.com`.
    pub client_id: String,
    /// The Desktop client's non-confidential secret, which Google's token
    /// endpoint requires on both the code exchange and the refresh even
    /// under PKCE.
    pub client_secret: Option<String>,
}

/// Google's registration for this build, or `None` when it carries none.
#[must_use]
pub fn google() -> Option<GoogleClient> {
    let client_id = present(option_env!("ASTRA_GOOGLE_DESKTOP_CLIENT_ID"))?;
    Some(GoogleClient {
        client_id: client_id.to_owned(),
        client_secret: present(option_env!("ASTRA_GOOGLE_DESKTOP_CLIENT_SECRET"))
            .map(str::to_owned),
    })
}

/// Microsoft's client id for this build, or `None` when it carries none.
#[must_use]
pub fn microsoft_client_id() -> Option<String> {
    present(option_env!("ASTRA_MS_CLIENT_ID")).map(str::to_owned)
}

/// A variable that was set to something, trimmed. Unset and blank are the
/// same answer.
fn present(value: Option<&'static str>) -> Option<&'static str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod credentials_tests;
