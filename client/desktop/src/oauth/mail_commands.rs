// SPDX-License-Identifier: GPL-3.0-only
//! The mail sign-in command surface: the probe the wizard's IMAP step
//! asks before it draws its screens (the server's own word over one
//! bounded, fail-soft question), the begin's session half (the minted
//! URL through the external arm, the spawned completion), and the
//! outcome the completion emits. The route walk itself lives in
//! [`super::mail_begin`]. The JMAP command surface's model: the question
//! is a convenience the password field beside it always outlives, so
//! every refusal, miss, and bound-out answer is the honest screen,
//! never an error.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use astra_account::AccountConfig;

use crate::runtime::AppRuntime;
use crate::AppState;

use super::commands::{OAuthCommandError, RefusalKind, SignInSurface};
use super::mail_begin::{production_mail_begin, MailFlowHalves};

/// How long the wizard's ask may run: the probe's own capability budget
/// and the issuer search's bound are both inside it, and the card stays
/// inert until the answer lands, so the whole question is bounded here
/// too.
pub(crate) const MAIL_OAUTH_BOUND: Duration = Duration::from_secs(10);

/// The wizard's three answers, as the wire carries them: the shape of
/// each screen the IMAP step draws. Mirrors [`astra_account::ImapAuth`],
/// which is not serde and stays in the account layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImapAuthView {
    /// The sign-in screen: the button names the issuer, the password
    /// demotes behind it (never leaves).
    SignIn {
        /// The authorization server the button would send the person to.
        issuer: String,
        /// The provider's own name for the button, when this build holds
        /// a static registration that names one.
        provider_label: Option<String>,
        /// Whether the server also accepts a password, so the demoted
        /// field is offered rather than hidden.
        password_also_works: bool,
    },
    /// The server takes a token but only from an application it
    /// registered in advance, and this build holds no registration.
    RegistrationNeeded {
        /// Whether a password still works, per the server's own word.
        password_also_works: bool,
    },
    /// No OAuth mechanism, or no answer: the password form as today.
    Password,
}

impl From<astra_account::ImapAuth> for ImapAuthView {
    fn from(answer: astra_account::ImapAuth) -> Self {
        match answer {
            astra_account::ImapAuth::SignIn {
                issuer,
                provider_label,
                password_also_works,
            } => Self::SignIn {
                issuer,
                provider_label,
                password_also_works,
            },
            astra_account::ImapAuth::RegistrationNeeded {
                password_also_works,
            } => Self::RegistrationNeeded {
                password_also_works,
            },
            astra_account::ImapAuth::Password => Self::Password,
        }
    }
}

/// The wizard's question, as the ask seam receives it: the config the
/// step's fields describe (the same walk the test connection takes, the
/// accepted certificate pinned in) beside the issuer the provider's own
/// autoconfig named, when detection carried one across.
pub(crate) struct MailAuthAsk {
    /// The config the step's fields describe: the dial address, the
    /// security, the login name, and the accepted certificate.
    pub(crate) config: AccountConfig,
    /// The issuer the provider's own autoconfig named, the first
    /// candidate the issuer chain tries.
    pub(crate) oauth_issuer: Option<String>,
}

/// Builds the config the wizard's ask runs over: the same
/// `build_config_toml` walk the step's test connection takes, so the
/// certificate the person accepted pins into the probe's TLS exactly as
/// it pins into the dial, over a credential of `grant` because the ask
/// presents nothing and the typed password may still be blank. `None`
/// for a setup that cannot build; the ask then never runs.
fn ask_config(setup: serde_json::Value) -> Option<AccountConfig> {
    let mut setup: astra_account::AccountSetup = serde_json::from_value(setup).ok()?;
    // The question is asked before anything is presented, so no password
    // is required to ask it; a grant credential builds the same server
    // sections the typed password would.
    setup.credential = astra_account::SetupCredential::Grant;
    let document = astra_account::build_config_toml(&setup).ok()?;
    astra_account::load_str(&document).ok()
}

/// Runs the wizard's question under `bound` and fails soft in both
/// directions: a setup that cannot build, a probe that never answers,
/// and a server that advertises nothing all cross as the password
/// screen, never an error, because the password field works everywhere
/// and the question is a convenience.
pub(crate) fn imap_auth_offer_with<F, Fut>(
    runtime: &AppRuntime,
    bound: Duration,
    setup: serde_json::Value,
    oauth_issuer: Option<String>,
    probe: F,
) -> ImapAuthView
where
    F: FnOnce(MailAuthAsk) -> Fut + Send,
    Fut: Future<Output = astra_account::ImapAuth> + Send,
{
    let Some(config) = ask_config(setup) else {
        return ImapAuthView::Password;
    };
    let ask = MailAuthAsk {
        config,
        oauth_issuer,
    };
    runtime
        .block_on(|| async {
            tokio::time::timeout(bound, probe(ask))
                .await
                .unwrap_or(astra_account::ImapAuth::Password)
        })
        .into()
}

/// How one begun mail sign-in ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MailSignInOutcome {
    /// The sign-in completed into an account: the address the flow began
    /// with.
    Added {
        /// The account's own address, as the surface names it.
        address: String,
    },
    /// The flow refused, with the classification word and the host's own
    /// sentence as the diagnostic detail.
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
pub(crate) struct MailSignInResult {
    /// How the flow ended.
    pub(crate) outcome: MailSignInOutcome,
}

/// The event the mail completion arrives on, the name the IMAP step
/// subscribes to.
pub(crate) const IMAP_SIGN_IN_RESULT_EVENT: &str = "imap_sign_in_result";

/// Begins one mail sign-in: builds the config the step's fields
/// describe, walks the route under its bound, opens the minted
/// authorization URL in the system browser through the session's
/// external arm, and returns once the browser opened. The completion
/// runs on the runtime: the spawned half waits for the redirect, spends
/// the flow through the claim, and hands the pending handle and the
/// redirect to the completion, whose outcome crosses through `emit`,
/// exactly the JMAP begin's shape.
///
/// # Errors
///
/// Answers [`OAuthCommandError::Discover`] when the setup cannot build,
/// the route refused or outlived its bound (each leaves nothing in
/// flight, no browser opened), [`OAuthCommandError::InFlight`] when a
/// sign-in already stands, and [`OAuthCommandError::Browser`] when the
/// browser refused to open.
pub(crate) fn mail_begin_with<E>(
    surface: &Arc<SignInSurface>,
    runtime: &AppRuntime,
    emit: E,
    halves: MailFlowHalves,
    setup: serde_json::Value,
    oauth_issuer: Option<String>,
) -> Result<(), OAuthCommandError>
where
    E: FnOnce(MailSignInResult) + Send + 'static,
{
    let Some(config) = ask_config(setup) else {
        return Err(OAuthCommandError::Discover(
            "the step's fields do not describe a server yet".to_owned(),
        ));
    };
    let MailFlowHalves {
        begin,
        complete,
        bound,
    } = halves;
    let start = runtime.block_on(|| async {
        match tokio::time::timeout(bound, begin(config, oauth_issuer)).await {
            Ok(start) => start.map_err(|err| OAuthCommandError::Discover(err.to_string())),
            Err(_elapsed) => Err(OAuthCommandError::Discover(format!(
                "the sign-in route timed out after {bound:?}"
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
            Err(err) => MailSignInOutcome::Refused {
                refusal: RefusalKind::Indeterminate,
                detail: err.to_string(),
            },
            Ok(raw) => {
                // The slot is spent before the completion runs, exactly
                // the session's own completion spends a minted one, so a
                // refused completion leaves the next begin fresh.
                if let Err(err) = desk.session().claim_external_sign_in() {
                    MailSignInOutcome::Refused {
                        refusal: RefusalKind::Indeterminate,
                        detail: err.to_string(),
                    }
                } else {
                    complete(start.pending, raw).await
                }
            }
        };
        emit(MailSignInResult { outcome });
    });
    Ok(())
}

/// Pulls the wizard's question for the step's fields: the three answers,
/// bounded and fail-soft in both directions. Never fails; a bound-out
/// or refusing question is the password screen.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn imap_auth_offer(
    state: State<'_, AppState>,
    setup: serde_json::Value,
    oauth_issuer: Option<String>,
) -> ImapAuthView {
    imap_auth_offer_with(
        &state.runtime,
        MAIL_OAUTH_BOUND,
        setup,
        oauth_issuer,
        |ask| async move {
            let Ok(tls) = astra_account::account_tls(&ask.config) else {
                return astra_account::ImapAuth::Password;
            };
            let Some(imap) = ask.config.imap.as_ref() else {
                return astra_account::ImapAuth::Password;
            };
            let detected = astra_account::detected_issuer(ask.oauth_issuer);
            let query = astra_account::ImapAuthQuery {
                imap_host: imap.addr.clone(),
                imap_security: imap.security,
                email: imap.username.clone(),
                detected: Some(&detected),
            };
            astra_account::imap_auth_offer(&query, &tls).await
        },
    )
}

/// Begins one mail sign-in: walks the route, registers this install, and
/// opens the authorization URL in the system browser. The completion
/// arrives later, as the [`IMAP_SIGN_IN_RESULT_EVENT`] event.
///
/// # Errors
///
/// Answers [`OAuthCommandError`] when the setup cannot build, the route
/// refused or timed out, a sign-in already stands, or the browser
/// refused to open; each leaves nothing new in flight.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn imap_oauth_begin(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    setup: serde_json::Value,
    oauth_issuer: Option<String>,
) -> Result<(), OAuthCommandError> {
    let emit = move |result: MailSignInResult| {
        use tauri::Emitter;
        let _ = app.emit(IMAP_SIGN_IN_RESULT_EVENT, result);
    };
    let halves = MailFlowHalves {
        begin: production_mail_begin(),
        complete: super::mail_add::production_mail_completion(&state, &state.runtime),
        bound: MAIL_OAUTH_BOUND,
    };
    mail_begin_with(
        &state.oauth,
        &state.runtime,
        emit,
        halves,
        setup,
        oauth_issuer,
    )
}

#[cfg(test)]
#[path = "mail_commands_tests.rs"]
mod tests;
