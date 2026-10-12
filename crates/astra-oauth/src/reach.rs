// SPDX-License-Identifier: GPL-3.0-only
//! How far a **failed** token request got: the one question a caller must
//! answer before retrying a refresh.
//!
//! Its own module because the answer is a small, self-contained
//! classification over [`OAuthError`](crate::OAuthError) with
//! consequences out of all proportion to its size: get it wrong in one
//! direction and an account waits a minute it did not need to; wrong in
//! the other and a replayed refresh token costs the user their whole
//! grant.

use crate::OAuthError;

/// How far a **failed** token request got: whether the refresh token it
/// presented may already have been consumed by the server.
///
/// This is the one question a caller must answer before retrying a
/// refresh, and no single error message answers it. A server that
/// rotates the refresh token on every refresh invalidates the presented
/// one the moment it answers: so a retry after a failure that *did*
/// reach the server presents a spent token. That is a replay, and a
/// server that treats a replay as theft (a ratcheting server answers
/// `invalid_grant` and revokes the whole grant) costs the user a full
/// re-authentication. A retry after a failure that provably never left
/// the device costs nothing.
///
/// The mapping is deliberately **asymmetric**:
/// [`TokenRequestReach::NotSent`] is returned only where it can be
/// proven, and everything unclear is
/// [`TokenRequestReach::MaybeProcessed`]. Being needlessly cautious
/// costs a delay; being wrong in the other direction costs the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRequestReach {
    /// The request provably never left this device: no connection was
    /// established, so no server saw the refresh token. Presenting it
    /// again is not a replay.
    NotSent,
    /// The request may have been processed. Either it was written to an
    /// established connection and the outcome is unknown, or the server
    /// demonstrably answered and we could not read the answer. The
    /// presented refresh token may be spent.
    MaybeProcessed,
}

impl OAuthError {
    /// Whether the failed request could have been processed by the
    /// server; see [`TokenRequestReach`]. Callers use this to decide
    /// whether re-presenting the same refresh token is safe or is a
    /// replay.
    #[must_use]
    pub fn reach(&self) -> TokenRequestReach {
        match self {
            // No request was ever built, so none was sent.
            Self::Tls(_) | Self::Callback(_) => TokenRequestReach::NotSent,
            // `is_connect` covers the failures that happen *before* a
            // byte of the request is written: DNS (a resolver that
            // answers nothing), a refused or unreachable peer, and a
            // failed TLS handshake. `is_builder` never reached the
            // network at all. Everything else on the send path: a write
            // that failed mid-flight, a read timeout waiting for the
            // status line, is unknowable from here, so it is not claimed
            // as safe.
            Self::Transport(err) => {
                if err.is_connect() || err.is_builder() {
                    TokenRequestReach::NotSent
                } else {
                    TokenRequestReach::MaybeProcessed
                }
            }
            // The server answered in every one of these: `ResponseLost`
            // and `Decode` had a body (a `Decode` on a 2xx means the
            // rotation was in a body we could not parse), and `Endpoint`
            // is the server's own words. A 4xx rejection probably did not
            // consume the token and a 5xx may well have, but both arrive
            // here as `Endpoint`, and guessing between them is exactly
            // what this type refuses to do. `invalid_grant` is the one
            // case a caller handles before asking.
            Self::ResponseLost(_) | Self::Decode(_) | Self::Endpoint { .. } => {
                TokenRequestReach::MaybeProcessed
            }
        }
    }
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod reach_tests;
