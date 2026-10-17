// SPDX-License-Identifier: GPL-3.0-only
//! The JMAP sign-in command surface: the advertise-gated offer (the
//! cheap pre-flight the wizard's JMAP form asks before showing the
//! button), the begin that runs the standards discovery and opens the
//! browser through the one session's external arm, and the completion
//! notification the form subscribes to. Unlike the integrated
//! providers, every endpoint here is discovered from the person's own
//! server at begin, so the discovery's refusal is a first-class outcome
//! the form shows beside the password field that always works.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use astra_account::{AccountError, JmapLoginStart};

use crate::runtime::AppRuntime;
use crate::AppState;

use super::commands::{OAuthCommandError, RefusalKind, SignInSurface};
use super::session::OAUTH_REDIRECT_URI;

/// How one begun JMAP sign-in ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JmapSignInOutcome {
    /// The sign-in completed into an account: the address the flow
    /// began with. The calendar never withholds: it rides the session's
    /// own advertised capability, so there is no probe to answer.
    Added {
        /// The account's own address, as the surface names it.
        address: String,
    },
    /// The flow refused, with the classification word and the host's
    /// own sentence as the diagnostic detail.
    Refused {
        /// The ported classification of the refusal.
        refusal: RefusalKind,
        /// The host's own sentence, for the surface to show beside its
        /// typed one.
        detail: String,
    },
}

/// The completion notification's payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct JmapSignInResult {
    /// How the flow ended.
    pub(crate) outcome: JmapSignInOutcome,
}

/// The event the JMAP completion arrives on, the name the form
/// subscribes to.
pub(crate) const JMAP_SIGN_IN_RESULT_EVENT: &str = "jmap_sign_in_result";

/// The discovery seam: the address and the server URL in, the minted
/// authorization start out. The production seam is
/// [`astra_account::begin_jmap_login`].
pub(crate) type JmapBegin = Arc<
    dyn Fn(
            String,
            Option<String>,
        ) -> Pin<Box<dyn Future<Output = Result<JmapLoginStart, AccountError>> + Send>>
        + Send
        + Sync,
>;

/// The discovery over the real chain: the production seam, registering
/// against the platform's redirect URI.
#[must_use]
pub(crate) fn production_jmap_begin() -> JmapBegin {
    Arc::new(|email, server_url| {
        Box::pin(async move {
            astra_account::begin_jmap_login(&email, server_url.as_deref(), OAUTH_REDIRECT_URI).await
        })
    })
}

/// The completion a spawned JMAP half runs: the pending handle and the
/// raw redirect in, the outcome out. The production seam spends them on
/// [`astra_account::complete_jmap_login`] and the account half beside
/// it.
pub(crate) type JmapCompletion = Box<
    dyn FnOnce(String, String) -> Pin<Box<dyn Future<Output = JmapSignInOutcome> + Send>> + Send,
>;

/// The completion over the real exchange and the account half: the
/// document the exchange answers is added exactly as the wizard's
/// password leg adds one.
#[must_use]
pub(crate) fn production_jmap_completion(state: &AppState, runtime: &AppRuntime) -> JmapCompletion {
    let wiring = super::jmap_add::JmapAccountWiring::from_state(state, runtime);
    let dial = super::jmap_add::production_jmap_dial();
    Box::new(move |pending, callback| {
        Box::pin(async move {
            let document = match astra_account::complete_jmap_login(
                &pending,
                &callback,
                time::OffsetDateTime::now_utc(),
            )
            .await
            {
                Ok(document) => document,
                Err(err) => {
                    return JmapSignInOutcome::Refused {
                        refusal: match &err {
                            AccountError::SigninRejected(_) => RefusalKind::Dead,
                            _ => RefusalKind::Indeterminate,
                        },
                        detail: err.to_string(),
                    };
                }
            };
            match super::jmap_add::complete_jmap_signin(wiring, document, dial).await {
                Ok(address) => JmapSignInOutcome::Added { address },
                Err(failure) => JmapSignInOutcome::Refused {
                    refusal: failure.refusal,
                    detail: failure.detail,
                },
            }
        })
    })
}

/// Runs the offer's probe under `bound` and fails soft: a probe that
/// never answers, or a server that advertises nothing, crosses as a
/// plain `false`, never an error, because the offer is a convenience
/// and the password field beside it always works.
pub(crate) fn jmap_offered_with<F, Fut>(
    runtime: &AppRuntime,
    bound: Duration,
    email: &str,
    server_url: Option<&str>,
    probe: F,
) -> bool
where
    F: FnOnce(String, Option<String>) -> Fut + Send,
    Fut: Future<Output = bool> + Send,
{
    let email = email.to_owned();
    let server_url = server_url.map(str::to_owned);
    runtime.block_on(|| async {
        tokio::time::timeout(bound, probe(email, server_url))
            .await
            .unwrap_or(false)
    })
}

/// Begins one JMAP sign-in: runs the discovery under `bound`, opens the
/// minted authorization URL in the system browser through the session's
/// external arm, and returns once the browser opened. The completion
/// runs on the runtime: the spawned half waits for the redirect, spends
/// the flow through the claim, and hands the pending handle and the
/// redirect to `complete`, whose outcome crosses through `emit`.
///
/// # Errors
///
/// Answers [`OAuthCommandError::Discover`] when the discovery refused
/// or outlived its bound (each leaves nothing in flight, no browser
/// opened), [`OAuthCommandError::InFlight`] when a sign-in already
/// stands, and [`OAuthCommandError::Browser`] when the browser refused
/// to open.
/// The halves one begun JMAP sign-in runs over: the discovery seam, the
/// completion seam, and the bound the discovery runs under.
pub(crate) struct JmapFlowHalves {
    /// The begin seam: the address and server URL in, the minted
    /// authorization start out.
    pub(crate) begin: JmapBegin,
    /// The completion seam: the pending handle and the raw redirect in,
    /// the outcome out.
    pub(crate) complete: JmapCompletion,
    /// How long the discovery may run before the begin refuses.
    pub(crate) bound: Duration,
}

/// Begins one JMAP sign-in: runs the discovery under its bound, opens
/// the minted authorization URL in the system browser through the
/// session's external arm, and returns once the browser opened. The
/// completion runs on the runtime: the spawned half waits for the
/// redirect, spends the flow through the claim, and hands the pending
/// handle and the redirect to the completion, whose outcome crosses
/// through `emit`.
///
/// # Errors
///
/// Answers [`OAuthCommandError::Discover`] when the discovery refused
/// or outlived its bound (each leaves nothing in flight, no browser
/// opened), [`OAuthCommandError::InFlight`] when a sign-in already
/// stands, and [`OAuthCommandError::Browser`] when the browser refused
/// to open.
pub(crate) fn jmap_begin_with<E>(
    surface: &Arc<SignInSurface>,
    runtime: &AppRuntime,
    emit: E,
    halves: JmapFlowHalves,
    email: &str,
    server_url: Option<&str>,
) -> Result<(), OAuthCommandError>
where
    E: FnOnce(JmapSignInResult) + Send + 'static,
{
    let email = email.to_owned();
    let server_url = server_url.map(str::to_owned);
    let JmapFlowHalves {
        begin,
        complete,
        bound,
    } = halves;
    let start = runtime.block_on(|| async {
        match tokio::time::timeout(bound, begin(email.clone(), server_url.clone())).await {
            Ok(start) => start.map_err(|err| OAuthCommandError::Discover(err.to_string())),
            Err(_elapsed) => Err(OAuthCommandError::Discover(format!(
                "the sign-in discovery timed out after {bound:?}"
            ))),
        }
    })?;
    let flow = surface
        .session()
        .begin_external_sign_in(&start.authorization_url)
        .map_err(|err| match err {
            super::session::SignInError::InFlight | super::session::SignInError::Cancelled => {
                OAuthCommandError::InFlight
            }
            super::session::SignInError::Browser(sentence) => OAuthCommandError::Browser(sentence),
            // The wait owns the capture's own failures; begin answers
            // before any wait runs.
            other => OAuthCommandError::Browser(other.to_string()),
        })?;
    let desk = Arc::clone(surface);
    runtime.spawn(async move {
        let outcome = match desk.session().wait_for_callback(flow).await {
            // The flow was cancelled while its redirect was pending; it
            // owes the surface nothing.
            Err(super::session::SignInError::Cancelled) => return,
            Err(err) => JmapSignInOutcome::Refused {
                refusal: RefusalKind::Indeterminate,
                detail: err.to_string(),
            },
            Ok(raw) => {
                // The slot is spent before the completion runs, exactly
                // the session's own completion spends a minted one, so a
                // refused completion leaves the next begin fresh.
                if let Err(err) = desk.session().claim_external_sign_in() {
                    JmapSignInOutcome::Refused {
                        refusal: RefusalKind::Indeterminate,
                        detail: err.to_string(),
                    }
                } else {
                    complete(start.pending, raw).await
                }
            }
        };
        emit(JmapSignInResult { outcome });
    });
    Ok(())
}

/// Pulls whether the JMAP server for the address offers discoverable
/// sign-in: the advertise gate the form's button renders behind. Never
/// fails; a bound-out or refusing probe offers nothing.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn jmap_oauth_offered(
    state: State<'_, AppState>,
    email: String,
    server_url: Option<String>,
) -> bool {
    jmap_offered_with(
        &state.runtime,
        crate::detection::DETECT_BOUND,
        &email,
        server_url.as_deref(),
        |email, server_url| async move {
            astra_account::jmap_signin_offered(&email, server_url.as_deref()).await
        },
    )
}

/// Begins one JMAP sign-in: discovers the server's authorization server,
/// registers this install, and opens the authorization URL in the system
/// browser. The completion arrives later, as the
/// [`JMAP_SIGN_IN_RESULT_EVENT`] event.
///
/// # Errors
///
/// Answers [`OAuthCommandError`] when the discovery refused or timed
/// out, a sign-in already stands, or the browser refused to open; each
/// leaves nothing new in flight.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn jmap_oauth_begin(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    email: String,
    server_url: Option<String>,
) -> Result<(), OAuthCommandError> {
    let emit = move |result: JmapSignInResult| {
        use tauri::Emitter;
        let _ = app.emit(JMAP_SIGN_IN_RESULT_EVENT, result);
    };
    let halves = JmapFlowHalves {
        begin: production_jmap_begin(),
        complete: production_jmap_completion(&state, &state.runtime),
        bound: crate::detection::DETECT_BOUND,
    };
    jmap_begin_with(
        &state.oauth,
        &state.runtime,
        emit,
        halves,
        &email,
        server_url.as_deref(),
    )
}

#[cfg(test)]
#[path = "jmap_commands_tests.rs"]
mod tests;
