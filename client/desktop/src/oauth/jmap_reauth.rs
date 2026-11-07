// SPDX-License-Identifier: GPL-3.0-only
//! The JMAP re-grant command surface: the expired grant's own remedy,
//! re-running the SAME authorization against the account's persisted grant
//! (no re-discovery, no re-registration) through the one session's external
//! arm, and swapping the fresh grant in place through the credential
//! transaction, so the account keeps its identity, its settings, and its
//! downloaded mail. The begin reads the stored `[jmap]` section from the
//! dial table; the completion re-reads it (the mailbox the same-address
//! rule compares cannot change mid-flow), exchanges the redirect through
//! the account layer's re-auth arm, and hands the fresh document to the
//! same account half a first sign-in runs, whose re-consent path is the
//! in-place swap itself: keep the stored shape, dial, persist through the
//! rotation path, re-join, and clear the standing refusal.
//!
//! The Graph one-tap's shape exactly: one command, the browser opens, the
//! outcome crosses later as one event the surface subscribes to while it
//! stands. A password account has no browser flow to re-run (the account
//! layer's own sentence says so), and an id the dial table does not hold
//! has nothing to re-grant at all.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tauri::State;

use astra_account::{AccountError, JmapAccount, JmapLoginStart};
use engine_api::AccountId;

use crate::runtime::AppRuntime;
use crate::AppState;

use super::commands::{OAuthCommandError, RefusalKind, SignInSurface};
use super::jmap_add::{complete_jmap_signin, JmapAccountWiring, JmapDial};
use super::jmap_commands::{JmapCompletion, JmapSignInOutcome, JmapSignInResult};

/// The event the re-grant's completion arrives on, the name the expired
/// row subscribes to.
pub(crate) const JMAP_REAUTH_RESULT_EVENT: &str = "jmap_reauth_result";

/** The re-grant result the completion event carries: the outcome beside
 * the account it belongs to, so a refusal lands its sentence beside its
 * own row and never another's (tokens from one provider tend to die
 * together, and every JMAP row hears this one event). */
#[derive(Clone, serde::Serialize)]
pub(crate) struct JmapReauthResult {
    /// The account the re-grant ran for, the row it names.
    pub(crate) account: String,
    /// How the flow ended.
    pub(crate) outcome: JmapSignInOutcome,
}

/// The begin seam: the stored section in, the minted authorization start
/// out. The production seam is [`astra_account::begin_jmap_reauth`],
/// which makes no network calls and re-registers nothing.
pub(crate) type JmapReauthBegin =
    Arc<dyn Fn(&JmapAccount) -> Result<JmapLoginStart, AccountError> + Send + Sync>;

/// The begin over the account layer's own arm: the production seam.
#[must_use]
pub(crate) fn production_jmap_reauth_begin() -> JmapReauthBegin {
    Arc::new(astra_account::begin_jmap_reauth)
}

/// The exchange seam: the stored section, the pending handle, and the raw
/// redirect in, the fresh section out. The production seam is
/// [`astra_account::complete_jmap_reauth`], which enforces the
/// same-address rule and needs the scripted server shape only its own
/// tests hold.
pub(crate) type JmapReauthExchange = Arc<
    dyn Fn(
            &JmapAccount,
            String,
            String,
        ) -> Pin<Box<dyn Future<Output = Result<JmapAccount, AccountError>> + Send>>
        + Send
        + Sync,
>;

/// The exchange over the account layer's own arm: the production seam.
#[must_use]
pub(crate) fn production_jmap_reauth_exchange() -> JmapReauthExchange {
    Arc::new(|stored, pending, callback| {
        let stored = stored.clone();
        Box::pin(async move {
            astra_account::complete_jmap_reauth(
                &stored,
                &pending,
                &callback,
                time::OffsetDateTime::now_utc(),
            )
            .await
        })
    })
}

/// Maps an account-layer refusal to the outcome the row shows.
fn refused(err: &AccountError) -> JmapSignInOutcome {
    JmapSignInOutcome::Refused {
        refusal: match err {
            AccountError::SigninRejected(_) => RefusalKind::Dead,
            _ => RefusalKind::Indeterminate,
        },
        detail: err.to_string(),
    }
}

/// The re-grant's completion over the given seams: re-read the stored
/// section, exchange the redirect, and run the fresh document through the
/// same account half a first sign-in runs (whose re-consent path dials,
/// keeps the stored shape, persists through the rotation, and re-joins).
pub(crate) async fn complete_jmap_regrant(
    wiring: JmapAccountWiring,
    dial: JmapDial,
    exchange: JmapReauthExchange,
    stored: Option<JmapAccount>,
    pending: String,
    callback: String,
) -> JmapSignInOutcome {
    let Some(stored) = stored else {
        return refused(&AccountError::Jmap(
            "the account left the dial table while its re-grant was pending".to_owned(),
        ));
    };
    let fresh = match exchange(&stored, pending, callback).await {
        Ok(fresh) => fresh,
        Err(err) => return refused(&err),
    };
    let document = match astra_account::jmap_reauth_document(&fresh) {
        Ok(document) => document,
        Err(err) => return refused(&err),
    };
    match complete_jmap_signin(wiring, document, dial).await {
        Ok(address) => JmapSignInOutcome::Added { address },
        Err(failure) => JmapSignInOutcome::Refused {
            refusal: failure.refusal,
            detail: failure.detail,
        },
    }
}

/// The halves one begun re-grant runs over: the begin seam and the
/// completion seam.
pub(crate) struct JmapReauthHalves {
    /// The begin seam: the stored section in, the authorization start out.
    pub(crate) begin: JmapReauthBegin,
    /// The completion seam: the pending handle and the raw redirect in,
    /// the outcome out.
    pub(crate) complete: JmapCompletion,
}

/// Begins one JMAP re-grant: mints the authorization URL from the stored
/// grant, opens it in the system browser through the session's external
/// arm, and returns once the browser opened. The completion runs on the
/// runtime, exactly the JMAP begin's spawned shape, and its outcome
/// crosses through `emit`.
///
/// # Errors
///
/// Answers [`OAuthCommandError::NoGrant`] when the section stores no
/// grant (a password account has no sign-in to re-run) or the begin
/// refused, [`OAuthCommandError::InFlight`] when a sign-in already
/// stands, and [`OAuthCommandError::Browser`] when the browser refused to
/// open.
pub(crate) fn jmap_reauth_with<E>(
    surface: &Arc<SignInSurface>,
    runtime: &AppRuntime,
    emit: E,
    halves: JmapReauthHalves,
    section: &JmapAccount,
) -> Result<(), OAuthCommandError>
where
    E: FnOnce(JmapSignInResult) + Send + 'static,
{
    let JmapReauthHalves { begin, complete } = halves;
    let start = begin(section).map_err(|err| OAuthCommandError::NoGrant(err.to_string()))?;
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
                // The slot is spent before the completion runs, so a
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

/// The stored `[jmap]` section for `account`, as the dial table holds it.
fn stored_section(state: &AppState, account: &str) -> Option<JmapAccount> {
    state
        .connections
        .of(account)
        .and_then(|(config, _)| config.jmap)
}

/// Begins one JMAP re-grant for a stored account: the authorization URL
/// is minted from the persisted grant and opened in the system browser,
/// and the outcome arrives later as the
/// [`JMAP_REAUTH_RESULT_EVENT`] event.
///
/// # Errors
///
/// Answers [`OAuthCommandError::NoGrant`] when the account is not stored
/// or stores no OAuth grant, and the session's own refusals beyond it;
/// each leaves nothing new in flight.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn jmap_reauth_begin(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account: String,
) -> Result<(), OAuthCommandError> {
    let Some(section) = stored_section(&state, &account) else {
        return Err(OAuthCommandError::NoGrant(format!(
            "no stored OAuth grant for this account: {account}"
        )));
    };
    let id = AccountId::try_from(account.as_str())
        .map_err(|err| OAuthCommandError::NoGrant(err.to_string()))?;
    let emit = move |result: JmapSignInResult| {
        use tauri::Emitter;
        let _ = app.emit(
            JMAP_REAUTH_RESULT_EVENT,
            JmapReauthResult {
                account: account.to_owned(),
                outcome: result.outcome,
            },
        );
    };
    let wiring = super::jmap_add::JmapAccountWiring::from_state(&state, &state.runtime);
    let dial = super::jmap_add::production_jmap_dial();
    let exchange = production_jmap_reauth_exchange();
    let dials = Arc::clone(&state.connections);
    let halves = JmapReauthHalves {
        begin: production_jmap_reauth_begin(),
        complete: Box::new(move |pending, callback| {
            let stored = dials.of(id.as_str()).and_then(|(config, _)| config.jmap);
            Box::pin(async move {
                complete_jmap_regrant(wiring, dial, exchange, stored, pending, callback).await
            })
        }),
    };
    jmap_reauth_with(&state.oauth, &state.runtime, emit, halves, &section)
}

#[cfg(test)]
#[path = "jmap_reauth_tests.rs"]
mod tests;
