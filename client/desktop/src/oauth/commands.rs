// SPDX-License-Identifier: GPL-3.0-only
//! The wizard's sign-in command surface: the offer (which providers this
//! build carries a registration for), the begin/cancel pair, and the
//! completion notification the wizard subscribes to. Everything here is
//! a thin shape over the session and the pure client; the refusal
//! vocabulary is the ported classification, never a sentence matched at
//! a call site.
//!
//! # How the completion reaches the wizard
//!
//! The completion crosses as **one event**, `oauth_sign_in_result`
//! ([`SIGN_IN_RESULT_EVENT`]), emitted with the whole
//! [`SignInResult`] payload. That is the agent-panel and draft-pool
//! pattern (the host emits a typed payload for an occurrence that is
//! host-side; the owning surface subscribes while it lives), chosen over
//! the surface_changed pull loop honestly: that loop serves immutable
//! snapshots the **app core** publishes, the sign-in result is not one,
//! and the app core carries no OAuth type at all (the oauth module's
//! placement record). The ordering is safe by construction: the wizard
//! subscribes while it is open, the browser cannot open before begin
//! returns, and the redirect cannot arrive before the browser opened, so
//! no completion can precede the subscription.
//!
//! # What a success means
//!
//! A Microsoft sign-in completes into an account: the completion runs the
//! account half ([`super::graph_add`]), which names the account from the
//! token, dials it, persists the grant, and joins it to the app, and the
//! success outcome carries what the wizard's sentence shows (the address,
//! a withheld calendar). Every other provider this build can offer (Google)
//! still completes with the bare signed-in word, honestly: no account type
//! consumes its token set until its slice lands.
//!
//! # The wiring this module owns
//!
//! The Tauri-touching adapters (the opener-backed browser, the headless
//! surface, the Windows argv forwarding) live in [`super::wiring`],
//! split out for the file cap; the contracts under them are pinned by
//! the session and command tests.
//!
//! [`forward_oauth_callbacks`]: super::session::forward_oauth_callbacks

use std::future::Future;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use astra_oauth::{
    credentials, GrantRefusal, OAuthClient, OAuthError, OAuthProviderConfig, TokenSet,
    GOOGLE_SCOPES, MICROSOFT_GRAPH_SCOPES,
};

use crate::runtime::AppRuntime;
use crate::AppState;

use super::loopback;
use super::session::{
    CaptureSource, SignInError, SignInSession, SystemBrowser, OAUTH_REDIRECT_URI,
};

/// A provider whose sign-in this build can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderKind {
    /// Microsoft's identity platform over Graph.
    Microsoft,
    /// Google's identity platform.
    Google,
}

impl core::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Microsoft => "Microsoft",
            Self::Google => "Google",
        })
    }
}

/// The offer: which providers this build can sign in with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OAuthOfferView {
    /// The providers carrying a registration, in the enum's order.
    pub(crate) providers: Vec<ProviderKind>,
}

/// What `oauth_begin` answers once the browser opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BrowserOpened {
    /// The provider whose authorization page the browser opened.
    pub(crate) provider: ProviderKind,
}

/// Why a sign-in refused, as the ported classification reads it: the
/// word the wizard's refusal sentences hang off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RefusalKind {
    /// The grant is dead: expired, revoked, or withdrawn.
    Dead,
    /// The grant lives but lacks a scope this build asked for.
    Underscoped,
    /// Nothing was learned about any standing grant.
    Indeterminate,
}

impl From<GrantRefusal> for RefusalKind {
    fn from(refusal: GrantRefusal) -> Self {
        match refusal {
            GrantRefusal::Dead => Self::Dead,
            GrantRefusal::Underscoped => Self::Underscoped,
            GrantRefusal::Indeterminate => Self::Indeterminate,
        }
    }
}

/// How one begun sign-in ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SignInOutcome {
    /// The provider issued a token set that no account consumed: the
    /// account type this provider names arrives with its slice, and the
    /// surface says so honestly rather than claiming an account.
    SignedIn,
    /// The sign-in completed into an account: the address the lookup
    /// named, and whether the grant withholds the calendar.
    Added {
        /// The account's own address, as the surface names it.
        address: String,
        /// Whether the grant withholds the calendar (a scope-denied
        /// refusal on the calendar probe): the mail stands and the agenda
        /// waits for a fresh sign-in.
        calendar_withheld: bool,
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

/// The completion notification's payload: the provider and how its
/// sign-in ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SignInResult {
    /// The provider the begun flow signed in with.
    pub(crate) provider: ProviderKind,
    /// How the flow ended.
    pub(crate) outcome: SignInOutcome,
}

/// A command failure, serialised to the webview as its display string.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OAuthCommandError {
    /// The named provider carries no registration in this build, so its
    /// sign-in is not offered at all.
    #[error("this build carries no {0} registration; its sign-in is not offered")]
    NotOffered(ProviderKind),
    /// A sign-in already stands; one at a time.
    #[error("a sign-in is already in flight; one sign-in at a time")]
    InFlight,
    /// The system browser could not be opened, so nothing started.
    #[error("the system browser could not be opened: {0}")]
    Browser(String),
    /// The loopback capture a Google flow needs could not be started:
    /// its port could not be bound, its client could not be built, or
    /// its serve thread could not start; nothing stays in flight.
    #[error("the loopback capture could not be started: {0}")]
    Capture(String),
}

impl Serialize for OAuthCommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// The OAuth clients this build's registrations carry, one per provider
/// with a registration: the offer is exactly these keys, so a build
/// given no registration offers nothing and the wizard's provider routes
/// never render. A registration whose client cannot be built (its TLS
/// stack refused) is dropped with a warning rather than offered broken.
/// Google's registration documents a Desktop client whose redirect is a
/// loopback port bound per flow, so its boot-time client is the template
/// [`loopback::begin`] rebuilds with the bound port in the redirect;
/// the portless template is the URI shape the Cloud Console
/// registration carries.
fn registered_clients() -> Vec<(ProviderKind, Arc<OAuthClient>)> {
    let mut clients = Vec::new();
    if let Some(client_id) = credentials::microsoft_client_id() {
        match OAuthClient::new(OAuthProviderConfig::microsoft(
            &client_id,
            "common",
            OAUTH_REDIRECT_URI,
            MICROSOFT_GRAPH_SCOPES,
        )) {
            Ok(client) => clients.push((ProviderKind::Microsoft, Arc::new(client))),
            Err(err) => {
                log::warn!("oauth: the microsoft client could not be built; its sign-in is not offered: {err}");
            }
        }
    }
    if let Some(google) = credentials::google() {
        match OAuthClient::new(OAuthProviderConfig::google(
            google.client_id,
            google.client_secret,
            loopback::TEMPLATE_REDIRECT_URI,
            GOOGLE_SCOPES,
        )) {
            Ok(client) => clients.push((ProviderKind::Google, Arc::new(client))),
            Err(err) => {
                log::warn!("oauth: the google client could not be built; its sign-in is not offered: {err}");
            }
        }
    }
    clients
}

/// The host's OAuth surface: the sign-in session over the injected
/// browser and capture, beside the registered clients; one value, built
/// at boot, held by the managed state.
pub(crate) struct SignInSurface {
    /// The session every sign-in runs on: one flow at a time, whichever
    /// provider began it.
    session: SignInSession,
    /// The registered clients the offer answers from.
    clients: Vec<(ProviderKind, Arc<OAuthClient>)>,
}

impl SignInSurface {
    /// The production surface: the providers whose registration this
    /// build carries, and no others.
    #[must_use]
    pub(crate) fn new(
        browser: Arc<dyn SystemBrowser>,
        callbacks: mpsc::UnboundedReceiver<String>,
        callback_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self::with_clients(browser, callbacks, callback_tx, registered_clients())
    }

    /// The surface over given clients: the test seam that drives the
    /// command pair without a registration.
    #[must_use]
    pub(crate) fn with_clients(
        browser: Arc<dyn SystemBrowser>,
        callbacks: mpsc::UnboundedReceiver<String>,
        callback_tx: mpsc::UnboundedSender<String>,
        clients: Vec<(ProviderKind, Arc<OAuthClient>)>,
    ) -> Self {
        Self {
            session: SignInSession::new(browser, callbacks, callback_tx),
            clients,
        }
    }

    /// The offer: exactly the providers with a client.
    #[must_use]
    pub(crate) fn offer(&self) -> OAuthOfferView {
        OAuthOfferView {
            providers: self.clients.iter().map(|(kind, _)| *kind).collect(),
        }
    }

    /// The registered client for `provider`.
    fn client(&self, provider: ProviderKind) -> Option<Arc<OAuthClient>> {
        self.clients
            .iter()
            .find(|(kind, _)| *kind == provider)
            .map(|(_, client)| Arc::clone(client))
    }

    /// The session the commands drive.
    #[must_use]
    pub(crate) fn session(&self) -> &SignInSession {
        &self.session
    }
}

/// Maps the redirect wait's failure to the outcome the surface shows: a
/// capture that closed learned nothing about any grant, so the refusal is
/// indeterminate.
fn wait_outcome(err: &SignInError) -> SignInOutcome {
    SignInOutcome::Refused {
        refusal: RefusalKind::Indeterminate,
        detail: err.to_string(),
    }
}

/// Maps the completion's refusal to the outcome the surface shows: the
/// ported classification word beside the core's own sentence. A consent
/// withdrawn at the provider (`access_denied` on the front channel) is
/// neither of the two grant-refusal codes, so it crosses as indeterminate
/// with the code in the detail. A token set the provider issued crosses
/// as whatever completion the caller wired: the bare word for a provider
/// no account consumes, the added account for one that does.
fn refused_outcome(err: &OAuthError) -> SignInOutcome {
    SignInOutcome::Refused {
        refusal: RefusalKind::from(err.refusal()),
        detail: err.to_string(),
    }
}

/// Begins one provider sign-in: reserves the single flight, mints the
/// request over the provider's registered client, opens the system
/// browser, and returns the browser-opened answer once the browser is
/// open. The completion runs on the runtime: the spawned half waits for
/// the deep-link redirect, spends it on the core's complete, hands a
/// token set to `complete` (the account half for a provider this build
/// finishes into an account, the bare word otherwise), and gives the
/// [`SignInResult`] to `emit`, which the command wires to the event.
///
/// A cancelled flow's spawned half ends silently: its wait recognises
/// itself as displaced, hands any redirect that arrived meanwhile back
/// for the standing flow, and emits nothing.
///
/// # Errors
///
/// Returns [`OAuthCommandError::NotOffered`] when the provider carries
/// no registration, [`OAuthCommandError::InFlight`] when a sign-in
/// already stands, and [`OAuthCommandError::Browser`] when the browser
/// refused to open; each leaves nothing new in flight.
pub(crate) fn begin_with<C, CFut, E>(
    surface: &Arc<SignInSurface>,
    runtime: &AppRuntime,
    emit: E,
    complete: C,
    provider: ProviderKind,
    login_hint: Option<&str>,
) -> Result<BrowserOpened, OAuthCommandError>
where
    E: FnOnce(SignInResult) + Send + 'static,
    C: FnOnce(TokenSet, OAuthProviderConfig) -> CFut + Send + 'static,
    CFut: Future<Output = SignInOutcome> + Send,
{
    let client = surface
        .client(provider)
        .ok_or(OAuthCommandError::NotOffered(provider))?;
    // A Google Desktop flow's redirect is the loopback port its listener
    // binds per flow, so its client is minted per flow over the
    // registration's template with the bound port in the redirect;
    // Microsoft's rides the registered custom scheme and the shared
    // deep-link capture.
    let (client, capture) = match provider {
        ProviderKind::Google => {
            let flow = loopback::begin(&client, loopback::CAPTURE_BOUND)
                .map_err(OAuthCommandError::Capture)?;
            (flow.client, CaptureSource::Loopback(flow.capture))
        }
        ProviderKind::Microsoft => (client, CaptureSource::DeepLink),
    };
    let flow = surface
        .session()
        .begin_sign_in(&client, login_hint, capture)
        .map_err(|err| match err {
            SignInError::InFlight | SignInError::Cancelled => OAuthCommandError::InFlight,
            SignInError::Browser(sentence) => OAuthCommandError::Browser(sentence),
            // The wait owns the capture's own failures; begin answers
            // before any wait runs.
            SignInError::CaptureClosed => OAuthCommandError::Browser(
                "the deep-link capture closed before the flow could begin".to_owned(),
            ),
            SignInError::LoopbackFailed(_) => OAuthCommandError::Browser(
                "the loopback capture failed before the flow could begin".to_owned(),
            ),
        })?;
    let desk = Arc::clone(surface);
    // The registration the flow's client carries: the document the
    // completion builds names the client id, tenant, redirect, and scopes
    // the authorization actually ran over.
    let flow_provider = client.provider().clone();
    runtime.spawn(async move {
        let outcome = match desk.session().wait_for_callback(flow).await {
            // The flow was cancelled while its redirect was pending; the
            // redirect itself was handed back for whichever flow stands,
            // and a cancelled flow owes the surface nothing.
            Err(SignInError::Cancelled) => return,
            Err(err) => wait_outcome(&err),
            Ok(raw) => {
                let finished = desk
                    .session()
                    .complete_sign_in(&raw, OffsetDateTime::now_utc())
                    .await;
                match finished {
                    Ok(tokens) => complete(tokens, flow_provider).await,
                    Err(err) => refused_outcome(&err),
                }
            }
        };
        emit(SignInResult { provider, outcome });
    });
    Ok(BrowserOpened { provider })
}

/// Cancels the standing sign-in, whichever provider began it: the
/// in-flight pair comes back so a later begin starts fresh. Answers
/// whether a flow stood.
pub(crate) fn cancel_with(surface: &SignInSurface) -> bool {
    surface.session().cancel_sign_in()
}

/// The event the completion notification arrives on, the name the web
/// subscribes to.
pub(crate) const SIGN_IN_RESULT_EVENT: &str = "oauth_sign_in_result";

/// Pulls the offer: which providers this build can sign in with.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn oauth_offer(state: State<'_, AppState>) -> OAuthOfferView {
    state.oauth.offer()
}

/// Begins one provider sign-in: mints, opens the system browser, and
/// answers the browser-opened state. The completion arrives later, as
/// the [`SIGN_IN_RESULT_EVENT`] event.
///
/// # Errors
///
/// Returns [`OAuthCommandError`] when the provider carries no
/// registration, a sign-in already stands, or the browser refused to
/// open; each leaves nothing new in flight.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn oauth_begin(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provider: ProviderKind,
    login_hint: Option<String>,
) -> Result<BrowserOpened, OAuthCommandError> {
    let emit = move |result: SignInResult| {
        use tauri::Emitter;
        let _ = app.emit(SIGN_IN_RESULT_EVENT, result);
    };
    let complete = super::completion::outcome_completion(&state, &state.runtime, provider);
    begin_with(
        &state.oauth,
        &state.runtime,
        emit,
        complete,
        provider,
        login_hint.as_deref(),
    )
}

/// Cancels the standing sign-in, whichever provider began it.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn oauth_cancel(state: State<'_, AppState>) {
    cancel_with(&state.oauth);
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
