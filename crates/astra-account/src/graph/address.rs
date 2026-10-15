// SPDX-License-Identifier: GPL-3.0-only
//! The signed-in account's own address: the Graph `/me` read that names a
//! freshly authorised account, so the person is never asked to type what
//! the provider already knows.
//!
//! Prefers `mail` (the SMTP address) and falls back to `userPrincipalName`
//! (the sign-in name): some accounts have no `mail` set, and the two
//! disagree exactly there. The read needs only `User.Read`, which every
//! Graph sign-in this build asks for holds.

use crate::{setup_trust_policy, AccountError};

/// The Graph v1.0 root the lookup reads.
const GRAPH_V1_ROOT: &str = "https://graph.microsoft.com/v1.0";

/// The fields the read selects: the SMTP address and the sign-in name.
const SELECT: &str = "$select=mail,userPrincipalName";

/// Looks up the signed-in account's own email address via Graph
/// `GET /me?$select=mail,userPrincipalName`, authenticating with the bearer
/// access token the flow just obtained.
///
/// # Errors
///
/// Returns [`AccountError::Graph`] if the request fails, is non-2xx, or
/// returns no usable address: a sign-in whose account cannot be named does
/// not become an account, so the completion refuses with this in the
/// detail.
pub async fn fetch_primary_address(access_token: &str) -> Result<String, AccountError> {
    fetch_primary_address_at(GRAPH_V1_ROOT, access_token).await
}

/// The lookup against a chosen Graph root: the seam the offline tests
/// drive against a stand-in server, everything else identical.
///
/// # Errors
///
/// As [`fetch_primary_address`].
pub(crate) async fn fetch_primary_address_at(
    base: &str,
    access_token: &str,
) -> Result<String, AccountError> {
    // No account exists yet, so there are no accepted certificates to
    // honour: the same trust policy every pre-account dial runs under.
    let tls = engine_tls::client_config(&setup_trust_policy())
        .map_err(|err| AccountError::Graph(format!("me lookup client: {err}")))?;
    let http = tls
        .reqwest_builder()
        .build()
        .map_err(|err| AccountError::Graph(format!("me lookup client: {err}")))?;
    let resp = http
        .get(format!("{base}/me?{SELECT}"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|err| AccountError::Graph(format!("me lookup: {err}")))?;
    if !resp.status().is_success() {
        return Err(AccountError::Graph(format!(
            "me lookup: http {}",
            resp.status().as_u16()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|err| AccountError::Graph(format!("me lookup decode: {err}")))?;
    let address = body
        .get("mail")
        .and_then(serde_json::Value::as_str)
        .filter(|mail| !mail.is_empty())
        .or_else(|| {
            body.get("userPrincipalName")
                .and_then(serde_json::Value::as_str)
        })
        .ok_or_else(|| AccountError::Graph("me lookup returned no address".to_owned()))?;
    Ok(address.to_owned())
}

#[cfg(test)]
#[path = "address_tests.rs"]
mod tests;
