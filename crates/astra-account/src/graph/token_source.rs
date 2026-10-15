// SPDX-License-Identifier: GPL-3.0-only
//! The shared, self-refreshing source of Microsoft Graph access tokens for
//! one account.
//!
//! Graph access tokens live about an hour, far shorter than an app session,
//! so every provider an account dials shares one [`GraphTokenSource`] that
//! mints a fresh access token (refreshing from the stored refresh token
//! when the cached one is stale) on demand, and reports a rotated refresh
//! token to the host via [`TokenSink`] to be re-persisted in the OS
//! keystore.
//!
//! The account's **concurrency ceiling** deliberately does not live here: a
//! bound per source would mean one ceiling per source against a server that
//! counts requests per account. The bound lives where every request passes,
//! in the engine's `RequestGate` built per account by [`crate::throttle`],
//! and [`GraphTokenSource::retry`] is how a dial reaches it.

use std::sync::Arc;

use astra_oauth::{GrantedScopes, OAuthClient, OAuthProviderConfig, TokenRequestReach};
use async_trait::async_trait;
use engine_api::{AccountId, Affiliation};
use time::{Duration, OffsetDateTime};

use crate::{account_log_handle, AccountError};

mod failure;
mod shared;

use failure::{
    FailureKind, RefreshFailure, DEAD_GRANT_COOLDOWN, MAYBE_PROCESSED_COOLDOWN, NOT_SENT_COOLDOWN,
};
pub use shared::CredentialOrigin;
use shared::{credential_for, SharedCredential};

/// Refresh the access token this long **before** it actually expires, so a
/// token never dies mid-request.
const REFRESH_SKEW: Duration = Duration::minutes(5);

/// A host sink for a **rotated** refresh token: a provider may return a new
/// refresh token on each refresh and eventually invalidate the old one, so
/// the host must overwrite the stored config in its OS keystore.
///
/// The trait lives here, beside the only caller, so the account layer stays
/// host-agnostic: `astra-account` defines and drives the seam, and the
/// desktop implements it over its credential transaction's rotation path.
/// `None` (tests, or a host that has not wired one) keeps the rotation in
/// memory only.
#[async_trait]
pub trait TokenSink: Send + Sync {
    /// Reports that `account`'s refresh token was rotated to
    /// `new_refresh_token`; the host re-serializes and re-persists that
    /// account's config.
    async fn refresh_token_rotated(&self, account: &AccountId, new_refresh_token: &str);

    /// Reports the scope set a refresh response named for `account`, on
    /// every refresh that named one. The host records it over the stored
    /// set when the two differ: the response is what the provider grants
    /// now, and consent withdrawn since the last one shows up nowhere else.
    async fn scopes_granted(&self, account: &AccountId, granted: &GrantedScopes);

    /// Reports who `account` belongs to, once the provider was asked.
    /// The host stores it with the account.
    async fn affiliation_found(&self, _account: &AccountId, _affiliation: &Affiliation) {}
}

/// A shared, self-refreshing source of Graph access tokens for one account.
pub struct GraphTokenSource {
    oauth: OAuthClient,
    pub(super) account: AccountId,
    /// Which provider family this source serves (`graph`): the shared type
    /// is provider-neutral, and a refresh log line that cannot say *whose*
    /// token it is answers half the question. Safe to log: it names the
    /// protocol and nothing else.
    pub(super) provider: &'static str,
    pub(super) sink: Option<Arc<dyn TokenSink>>,
    /// The account's credential state and its refresh single-flight,
    /// **shared with every other token source for this account in this
    /// process**, not owned.
    ///
    /// Owning it is the bug: without the single-flight, every provider that
    /// finds the access token stale posts its own refresh with the *same*
    /// refresh token; a server that rotates then sees a **replay**, and one
    /// that ratchets revokes the entire grant. Sharing the state closes
    /// that within one runtime and between two, with the same mechanism
    /// (see [`shared`]).
    credential: Arc<SharedCredential>,
}

impl core::fmt::Debug for GraphTokenSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never surface either token.
        f.debug_struct("GraphTokenSource")
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}

impl GraphTokenSource {
    /// Builds a token source for `provider`, minting access tokens from
    /// `refresh_token`. `sink` (optional) receives a rotated refresh token
    /// for re-persistence.
    ///
    /// Takes the provider half of a stored account config rather than the
    /// whole stored document: the stored Microsoft account shape belongs to
    /// the host, which pulls the refresh token out of its secure store and
    /// hands both halves here.
    ///
    /// # Errors
    ///
    /// Returns [`AccountError::Graph`] if the OAuth HTTP client cannot be
    /// built.
    pub fn new(
        provider: &OAuthProviderConfig,
        account: AccountId,
        refresh_token: &str,
        sink: Option<Arc<dyn TokenSink>>,
        origin: CredentialOrigin,
    ) -> Result<Arc<Self>, AccountError> {
        let oauth = OAuthClient::new(provider.clone())
            .map_err(|err| AccountError::Graph(err.to_string()))?;
        Ok(Self::from_parts(
            oauth,
            account,
            refresh_token.to_owned(),
            sink,
            "graph",
            origin,
        ))
    }

    /// Builds a token source from an already-constructed [`OAuthClient`]:
    /// the seam offline tests use to point the refresh at a mock token
    /// endpoint.
    #[must_use]
    pub fn from_parts(
        oauth: OAuthClient,
        account: AccountId,
        refresh_token: String,
        sink: Option<Arc<dyn TokenSink>>,
        provider: &'static str,
        origin: CredentialOrigin,
    ) -> Arc<Self> {
        let credential = credential_for(&account, refresh_token, origin);
        Arc::new(Self {
            oauth,
            account,
            provider,
            sink,
            credential,
        })
    }

    /// The throttling config every provider built from this source must
    /// use: the shared policy, gated on **this account's** ceiling.
    ///
    /// Here rather than at each call site because getting the account wrong
    /// is the entire failure mode, and this type is the one thing every one
    /// of an account's OAuth providers already shares. Two sources for one
    /// account resolve to the same gate.
    ///
    /// `pub` so a gated live suite can assert the account's ceiling was
    /// narrowed, rather than infer it from a server that happened not to
    /// complain.
    pub fn retry(&self) -> engine_api::RetryConfig {
        crate::throttle::account_retry(&self.account)
    }

    /// Seeds the cached access token + expiry; used right after the sign-in
    /// code exchange, whose fresh access token would otherwise be discarded
    /// and immediately re-refreshed on the first sync.
    pub fn seed_access_token(&self, access_token: String, expires_at: OffsetDateTime) {
        let mut state = self
            .credential
            .state
            .lock()
            .expect("token state mutex poisoned");
        state.access_token = access_token;
        state.expires_at = expires_at;
        // A freshly signed-in account must not inherit a cool-down from the credential it
        // replaced.
        state.last_failure = None;
    }

    /// A valid access token other than `refused`, which a server has just
    /// turned down.
    ///
    /// The cached token is dropped when it is still the refused one, so
    /// this refreshes even before the token's stated expiry. Callers
    /// renewing the same refused token share one refresh: the first
    /// replaces it, and the rest find the replacement cached.
    ///
    /// # Errors
    ///
    /// As [`access_token`](Self::access_token).
    pub async fn access_token_replacing(&self, refused: &str) -> Result<String, AccountError> {
        {
            let mut state = self
                .credential
                .state
                .lock()
                .expect("token state mutex poisoned");
            if state.access_token == refused {
                state.access_token.clear();
            }
        }
        self.access_token().await
    }

    /// The cached access token, when it is still comfortably valid at
    /// `now`. Never holds the lock across an `await`.
    fn cached(&self, now: OffsetDateTime) -> Option<String> {
        let state = self
            .credential
            .state
            .lock()
            .expect("token state mutex poisoned");
        (!state.access_token.is_empty() && now + REFRESH_SKEW < state.expires_at)
            .then(|| state.access_token.clone())
    }

    /// The remembered outcome of a recent failed refresh, when it is still
    /// inside its cool-down: `(reach, message, seconds still to wait)`.
    ///
    /// This is the counterpart of [`GraphTokenSource::cached`] for the
    /// failure path. Serializing the refresh made concurrent callers share
    /// a *success*; on a failure each waiter in turn would acquire the
    /// lock, find no cached token, and post its own refresh presenting the
    /// **same** refresh token. One failure therefore becomes one request
    /// per waiting provider, and if the first request did reach the
    /// server, every one after it is a replay of a spent token, which is
    /// what a ratcheting server revokes the grant over.
    fn recent_failure(&self, now: OffsetDateTime) -> Option<(FailureKind, String, Duration)> {
        let state = self
            .credential
            .state
            .lock()
            .expect("token state mutex poisoned");
        let failure = state.last_failure.as_ref()?;
        let until = failure.at + failure.kind.cooldown();
        (now < until).then(|| (failure.kind, failure.message.clone(), until - now))
    }

    /// Remembers a failed refresh so the callers behind it take this
    /// outcome rather than re-presenting the same refresh token.
    fn note_failure(&self, at: OffsetDateTime, kind: FailureKind, message: String) {
        let mut state = self
            .credential
            .state
            .lock()
            .expect("token state mutex poisoned");
        state.last_failure = Some(RefreshFailure { at, kind, message });
    }

    /// Test-only: how the last refresh failed, and how long it suppresses
    /// further attempts.
    #[cfg(test)]
    fn last_failure(&self) -> Option<(FailureKind, i64)> {
        let state = self
            .credential
            .state
            .lock()
            .expect("token state mutex poisoned");
        state
            .last_failure
            .as_ref()
            .map(|failure| (failure.kind, failure.kind.cooldown().whole_seconds()))
    }

    /// Test-only: backdates the remembered failure, so a cool-down can be
    /// driven past its end without a test sleeping through it.
    #[cfg(test)]
    fn backdate_failure(&self, by: Duration) {
        let mut state = self
            .credential
            .state
            .lock()
            .expect("token state mutex poisoned");
        if let Some(failure) = state.last_failure.as_mut() {
            failure.at -= by;
        }
    }

    /// Returns a valid access token, refreshing (and caching) it when the
    /// current one is missing or within `REFRESH_SKEW` of expiry. A refresh
    /// that rotates the refresh token updates the cache and notifies the
    /// [`TokenSink`].
    ///
    /// **Exactly one refresh runs at a time per account**, and every other
    /// caller waits for it and takes its result; its *failure* as readily
    /// as its success. That is not an optimisation: a second refresh with
    /// the same refresh token is a *replay*, and a ratcheting authorization
    /// server answers it by revoking the grant. Sharing only the success is
    /// not enough, because a failure leaves nothing cached and each waiter
    /// would go on to present that same token itself.
    ///
    /// # Errors
    ///
    /// Returns [`AccountError::SigninRejected`] if the refresh token is
    /// revoked or expired (re-authenticate), or [`AccountError::Graph`] on
    /// a transient refresh failure.
    pub async fn access_token(&self) -> Result<String, AccountError> {
        // Fast path: a cached token still comfortably valid, taken without serializing.
        if let Some(token) = self.cached(OffsetDateTime::now_utc()) {
            return Ok(token);
        }
        // Only one refresh at a time. Whoever waited here may find the winner already cached a
        // fresh token, in which case there is nothing left to do; re-check *after* acquiring,
        // or every waiter would go on to post the replay this lock exists to prevent.
        let _refreshing = self.credential.refreshing.lock().await;
        let now = OffsetDateTime::now_utc();
        if let Some(token) = self.cached(now) {
            log::debug!("graph: another caller refreshed while we waited; using its token");
            return Ok(token);
        }
        // And if the refresh we queued behind *failed*, take its failure. Posting our own
        // would present the same refresh token again, which is a replay whenever that request
        // reached the server. See `Self::recent_failure`.
        if let Some((kind, message, remaining)) = self.recent_failure(now) {
            log::debug!(
                "oauth: {} [{}]: a recent refresh already failed {}, so this caller takes that \
                 outcome instead of re-presenting the same refresh token; the next attempt is in \
                 {}s: {message}",
                self.provider,
                account_log_handle(self.account.as_str()),
                kind.describe(),
                remaining.whole_seconds(),
            );
            return Err(kind.error(&message));
        }
        let (refresh_token, had_token, attempts) = {
            let mut state = self
                .credential
                .state
                .lock()
                .expect("token state mutex poisoned");
            state.attempts += 1;
            (
                state.refresh_token.clone(),
                !state.access_token.is_empty(),
                state.attempts,
            )
        };
        // INFO, not DEBUG. Token refresh is the single most consequential thing this app does
        // that a person can neither see nor trigger: every field here is a fact about the
        // protocol, never an address or a token.
        let provider = self.provider;
        // Which account this is. Several accounts refresh concurrently on a real device, and
        // without a handle these lines interleave into an unreadable stream: three identical
        // rotation lines read the same whether that was three accounts once or one account
        // three times, and those are a healthy launch and a refresh loop. Never the account
        // id itself: see [`crate::account_log_handle`].
        let account_handle = account_log_handle(self.account.as_str());
        // The third arm is not cosmetic. A *failed* refresh leaves `access_token` empty, so
        // every later attempt would otherwise report "first use in this process", which reads
        // as a fresh, healthy launch on a log that is actually an account retrying.
        log::info!(
            "oauth: {provider} [{account_handle}]: refreshing the access token ({})",
            if had_token {
                "the cached one is within the expiry skew".to_owned()
            } else if attempts == 1 {
                "none cached; first use in this process".to_owned()
            } else {
                format!(
                    "still none cached; {} earlier attempt(s) in this process failed",
                    attempts - 1
                )
            },
        );
        let started = std::time::Instant::now();
        let tokens = match self.oauth.refresh(&refresh_token, now).await {
            Ok(tokens) => tokens,
            // Both refusals that never recover, and the line support reads first. A dead grant
            // (`invalid_grant`) and an under-scoped one (`invalid_scope`) mean different things
            // about the person (signed out versus still signed in) but for a token source they
            // are the same fact: this refresh token will not mint a token again, and only
            // consent changes that. Retrying either never ends.
            Err(err) if err.refusal().needs_reauth() => {
                log::warn!(
                    "oauth: {provider} [{account_handle}]: the server REFUSED the stored refresh token \
                     after {}ms: the sign-in cannot be renewed and only a fresh one \
                     helps, not asking again for {}m: {err}",
                    started.elapsed().as_millis(),
                    DEAD_GRANT_COOLDOWN.whole_minutes(),
                );
                // Remembered like any other failure. Nothing about a dead grant changes by
                // asking again, and every provider on the account is queued behind this one.
                self.note_failure(now, FailureKind::DeadGrant, err.to_string());
                return Err(AccountError::SigninRejected(err.to_string()));
            }
            Err(err) => {
                // The question that separates these arms is whether the server could have
                // *processed* the request; because if it did, it consumed the refresh token
                // presented and the replacement was in the answer that never arrived.
                // Re-presenting the old one is then a replay, and a ratcheting server revokes
                // the grant over it.
                let reach = err.reach();
                match reach {
                    TokenRequestReach::NotSent => log::warn!(
                        "oauth: {provider} [{account_handle}]: the token refresh could not leave this \
                         device after {}ms, so no server saw the refresh token and it is still \
                         good, not retrying for {}s: {err}",
                        started.elapsed().as_millis(),
                        NOT_SENT_COOLDOWN.whole_seconds(),
                    ),
                    TokenRequestReach::MaybeProcessed => log::warn!(
                        "oauth: {provider} [{account_handle}]: the token refresh MAY have reached the \
                         server after {}ms; if it did, the refresh token we presented is spent \
                         and its replacement was lost with the response. Holding off {}s rather \
                         than re-presenting it, because a replay is what revokes a grant: {err}",
                        started.elapsed().as_millis(),
                        MAYBE_PROCESSED_COOLDOWN.whole_seconds(),
                    ),
                }
                self.note_failure(now, FailureKind::Unanswered(reach), err.to_string());
                return Err(AccountError::Graph(format!("token refresh: {err}")));
            }
        };
        let access = tokens.access_token.expose().to_owned();
        let rotated = tokens
            .refresh_token
            .as_ref()
            .map(|secret| secret.expose().to_owned());
        {
            let mut state = self
                .credential
                .state
                .lock()
                .expect("token state mutex poisoned");
            state.access_token.clone_from(&access);
            state.expires_at = tokens.expires_at;
            state.last_failure = None;
            if let Some(new_refresh) = &rotated {
                state.refresh_token.clone_from(new_refresh);
            }
        }
        // Persist a rotated refresh token so the next launch uses the current one. Reported
        // even with no sink wired, because "the server rotated and nobody stored it" is the
        // single most consequential thing that can happen here: the account works for the
        // rest of the session on the token just minted, then fails at the next launch with
        // an `invalid_grant` that names nothing.
        log::info!(
            "oauth: {provider} [{account_handle}]: refreshed in {}ms; the access token is valid for {} \
             more minute(s), and the server {}",
            started.elapsed().as_millis(),
            (tokens.expires_at - OffsetDateTime::now_utc()).whole_minutes(),
            if rotated.is_some() {
                "ROTATED the refresh token"
            } else {
                "kept the same refresh token"
            },
        );
        match (self.sink.as_ref(), rotated.as_ref()) {
            (Some(sink), Some(new_refresh)) => {
                log::info!(
                    "oauth: [{account_handle}] the server rotated this account's refresh token; \
                     handing it to the host to re-persist",
                );
                sink.refresh_token_rotated(&self.account, new_refresh).await;
            }
            (None, Some(_)) => log::warn!(
                "oauth: [{account_handle}] the server rotated this account's refresh token but NO token \
                 sink is wired, so the new token is held in memory only: this account will fail \
                 to authenticate after the next restart",
            ),
            (_, None) => {}
        }
        if let (Some(sink), Some(granted)) = (
            self.sink.as_ref(),
            GrantedScopes::from_refresh(&tokens.scope),
        ) {
            sink.scopes_granted(&self.account, &granted).await;
        }
        Ok(access)
    }
}

/// Test-only helpers shared by this module's tests and the sibling provider
/// tests: the mock token endpoints (including the **ratcheting** one that
/// reproduces a replay-detecting server) and a [`GraphTokenSource`] pointed
/// at them. Its own file, because the mock servers grew past the point
/// where they belonged in the middle of the production type.
#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
