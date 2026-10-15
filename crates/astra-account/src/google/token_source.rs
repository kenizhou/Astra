// SPDX-License-Identifier: GPL-3.0-only
//! The Google account's token source: the shared, provider-neutral
//! [`GraphTokenSource`] built from the stored `[google]` document.
//!
//! The source type is Graph's shape (an OAuth client, a refresh token,
//! the single-flight, and the rotation sink), so Google's half is a
//! constructor, not a second implementation. The deltas it wraps live in
//! the document it reads: the Desktop client's non-confidential secret
//! rides the token endpoint on the exchange AND the refresh, so the
//! section persists it for a launch-later refresh to send again, and
//! Google does not rotate its refresh tokens in practice, so the
//! rotation write stays as the backstop that rarely fires but is not
//! optional.

use std::sync::Arc;

use astra_oauth::OAuthClient;
use engine_api::AccountId;

use crate::{AccountError, CredentialOrigin, GoogleAccount, GraphTokenSource, TokenSink};

/// Builds the shared, self-refreshing token source for a Google
/// account's stored document: the Google parallel of
/// [`GraphTokenSource::new`](crate::GraphTokenSource::new), which takes
/// the graph section. It reuses the same provider-neutral source, built
/// from the account's Google OAuth endpoints with the Desktop secret the
/// document carries; `sink` (optional) receives a rotated refresh token
/// for re-persistence (rare for Google, which does not rotate on a
/// refresh grant).
///
/// # Errors
///
/// Returns [`AccountError::Google`] if the OAuth HTTP client cannot be
/// built.
pub fn google_token_source(
    config: &GoogleAccount,
    account: AccountId,
    sink: Option<Arc<dyn TokenSink>>,
    origin: CredentialOrigin,
) -> Result<Arc<GraphTokenSource>, AccountError> {
    let oauth = OAuthClient::new(config.provider_config())
        .map_err(|err| AccountError::Google(err.to_string()))?;
    Ok(GraphTokenSource::from_parts(
        oauth,
        account,
        config.refresh_token.expose().to_owned(),
        sink,
        "google",
        origin,
    ))
}

#[cfg(test)]
#[path = "token_source_tests.rs"]
mod tests;
