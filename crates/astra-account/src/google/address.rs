// SPDX-License-Identifier: GPL-3.0-only
//! The signed-in Google account's own address: the profile read that
//! names a freshly authorised account, so the person is never asked to
//! type what the provider already knows.
//!
//! Every sign-in asks for `userinfo.email`, so the read runs against
//! `oauth2/v3/userinfo` (its `email` field). A grant issued before that
//! scope was asked for is named through the Gmail profile instead (its
//! `emailAddress` field), which the mail scope covers. The read needs no
//! account, so it rides the pre-account trust policy: there are no
//! accepted certificates to honour yet.

use astra_oauth::scopes::GOOGLE;

use crate::{setup_trust_policy, AccountError};

/// The Google userinfo root the lookup reads.
const GOOGLE_USERINFO_ROOT: &str = "https://www.googleapis.com";

/// The Gmail profile root: the naming fallback for a grant without
/// `userinfo.email`.
const GMAIL_PROFILE_ROOT: &str = "https://gmail.googleapis.com";

/// The scope the userinfo endpoint answers to.
const USERINFO_EMAIL_SCOPE: &str = "https://www.googleapis.com/auth/userinfo.email";

/// Looks up the signed-in account's own email address via the Google
/// userinfo read, authenticating with the bearer access token the flow
/// just obtained.
///
/// `granted` is what the grant carries. Every sign-in asks for
/// `userinfo.email`, which names the account whatever else the person
/// allowed; a grant without it is read through the Gmail profile, which
/// its mail scope covers.
///
/// # Errors
///
/// Returns [`AccountError::Google`] if the request fails, is non-2xx, or
/// returns no usable address: a sign-in whose account cannot be named does
/// not become an account, so the completion refuses with this in the
/// detail.
pub async fn fetch_google_address(
    access_token: &str,
    granted: &[String],
) -> Result<String, AccountError> {
    fetch_google_address_at(
        GOOGLE_USERINFO_ROOT,
        GMAIL_PROFILE_ROOT,
        access_token,
        granted,
    )
    .await
}

/// The lookup against chosen Google roots: the seam the offline proofs
/// drive against stand-in servers, everything else identical to the
/// constant-root call the completion makes.
///
/// # Errors
///
/// As [`fetch_google_address`].
pub async fn fetch_google_address_at(
    userinfo_root: &str,
    profile_root: &str,
    access_token: &str,
    granted: &[String],
) -> Result<String, AccountError> {
    let (url, field) = if GOOGLE.holds(granted, USERINFO_EMAIL_SCOPE) {
        (format!("{userinfo_root}/oauth2/v3/userinfo"), "email")
    } else {
        (
            format!("{profile_root}/gmail/v1/users/me/profile"),
            "emailAddress",
        )
    };
    // No account exists yet, so there are no accepted certificates to
    // honour: the same trust policy every pre-account dial runs under.
    let tls = engine_tls::client_config(&setup_trust_policy())
        .map_err(|err| AccountError::Google(format!("address lookup client: {err}")))?;
    let http = tls
        .reqwest_builder()
        .build()
        .map_err(|err| AccountError::Google(format!("address lookup client: {err}")))?;
    let resp = http
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|err| AccountError::Google(format!("address lookup: {err}")))?;
    if !resp.status().is_success() {
        return Err(AccountError::Google(format!(
            "address lookup: http {}",
            resp.status().as_u16()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|err| AccountError::Google(format!("address lookup decode: {err}")))?;
    let address = body
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|address| !address.is_empty())
        .ok_or_else(|| AccountError::Google("address lookup returned no address".to_owned()))?;
    Ok(address.to_owned())
}

#[cfg(test)]
#[path = "address_tests.rs"]
mod tests;
