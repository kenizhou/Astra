// SPDX-License-Identifier: GPL-3.0-only
//! The system-browser sign-in session: the host half of one
//! Authorization-Code-with-PKCE flow. The session mints nothing itself;
//! [`OAuthClient::begin`] builds the URL, the state, and the PKCE pair,
//! and the session's job is the platform half around them: open the URL
//! in the system browser (never an embedded webview, per the design's
//! D2), wait for the deep-link redirect the browser lands on the
//! registered scheme, and hand that raw URL back for
//! [`OAuthClient::complete`], where the `state` CSRF check stands.
//!
//! The two platform seams are injectable so the whole session is driven
//! in tests without a Tauri runtime: [`SystemBrowser`] (the shell's
//! opener in production, a counting stand-in in tests) and the callback
//! channel (the receiving end of the deep-link capture's forwarding; a
//! test feeds it directly). The capture's production shape, the
//! plugin's `on_open_url` event filtered by [`oauth_callback`], is one
//! function the tests drive with parsed URLs, so the filter's behaviour
//! is pinned exactly as the runtime delivers it.
//!
//! The client is a per-flow input, not session state: Microsoft and
//! Google flows share one session (one channel, one in-flight slot),
//! and each begin names the provider whose client mints it.
//!
//! [`OAuthClient::begin`]: astra_oauth::OAuthClient::begin
//! [`OAuthClient::complete`]: astra_oauth::OAuthClient::complete

use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use tokio::sync::mpsc;

use astra_oauth::{OAuthClient, OAuthError, TokenSet};

/// The custom scheme every OAuth callback redirect lands on: the app's
/// own identifier, the one `tauri.conf.json`'s deep-link block registers
/// beside `mailto`. The OS only delivers links on registered schemes, so
/// this is the contract between the provider's registered redirect URI
/// and the capture's filter.
pub(crate) const OAUTH_SCHEME: &str = "com.kenizhou.astra";

/// The redirect URI the provider configs register: the OAuth path on
/// this app's scheme, the shape `astra-oauth`'s callback parser names.
pub(crate) const OAUTH_REDIRECT_URI: &str = "com.kenizhou.astra://oauth";

/// Why a sign-in step refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SignInError {
    /// A sign-in is already in flight: one at a time, and the second
    /// begin says so rather than silently displacing the first.
    #[error("a sign-in is already in flight; one sign-in at a time")]
    InFlight,
    /// The system browser could not be opened, so no flow started and
    /// nothing stays in flight.
    #[error("the system browser could not be opened: {0}")]
    Browser(String),
    /// The deep-link capture closed before any redirect arrived, so the
    /// flow that opened the browser can never finish.
    #[error("the deep-link capture closed before the redirect arrived")]
    CaptureClosed,
    /// The flow this waiter was minted by was cancelled while it held
    /// the capture; a redirect that arrived meanwhile was handed back
    /// for the standing flow.
    #[error("the sign-in was cancelled while its redirect was pending")]
    Cancelled,
}

/// The browser the authorization URL opens in: the shell's opener in
/// production, a stand-in in tests. Opening the system browser (never an
/// embedded webview) is the whole of this seam; it knows nothing about
/// the flow around it.
pub(crate) trait SystemBrowser: Send + Sync {
    /// Opens `url` in the system browser.
    ///
    /// # Errors
    ///
    /// Returns the opener's failure as a sentence when the URL could not
    /// be handed to the platform.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// The OAuth-scheme callback among one deep-link delivery's URLs, whole.
///
/// A delivery is a batch (the plugin's event carries every URL the OS
/// handed over in one go); the capture keeps the one URL on
/// [`OAUTH_SCHEME`] and drops the rest, and returns it as the raw string
/// the sign-in hands to the core's parser. It never inspects the query:
/// `code`, `state`, and any provider `error` are `astra-oauth`'s to read.
#[must_use]
pub(crate) fn oauth_callback(urls: &[url::Url]) -> Option<String> {
    urls.iter()
        .find(|url| url.scheme() == OAUTH_SCHEME)
        .map(ToString::to_string)
}

/// Wires the deep-link plugin's event stream into `tx`: every OAuth
/// callback the running app receives is forwarded whole. The sign-in
/// wiring calls this once at setup; it is here, beside the filter it
/// applies, so the capture's production shape lives in one place.
///
/// On Windows, a deep link that arrives while the app is already running
/// reaches the first instance through the single-instance callback's
/// argv, not this event; that forwarding (`DeepLink::handle_cli_arguments`)
/// is the sign-in commands' wiring too, recorded in the module doc.
pub(crate) fn forward_oauth_callbacks<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    tx: mpsc::UnboundedSender<String>,
) {
    use tauri_plugin_deep_link::DeepLinkExt;

    app.deep_link().on_open_url(move |event| {
        if let Some(raw) = oauth_callback(&event.urls()) {
            let _ = tx.send(raw);
        }
    });
}

/// One in-flight flow's held half: the client that minted it, the
/// `state` the callback must echo, and the PKCE verifier the exchange
/// sends. Held from [`SignInSession::begin_sign_in`] minted it until
/// [`SignInSession::complete_sign_in`] spent it, so a second begin can
/// be refused honestly.
struct InFlight {
    /// Which flow this is: minted from the session's counter, so a
    /// waiter abandoned by a cancel can tell its own redirect from the
    /// next flow's and hand the latter back rather than steal it.
    flow: u64,
    /// The client whose flow this is; the completion's exchange runs on
    /// it, so the flow spends the client it was minted by.
    client: Arc<OAuthClient>,
    /// The CSRF state issued at begin; the callback must echo it.
    state: String,
    /// The PKCE verifier paired with the challenge the authorization
    /// request carried.
    verifier: String,
}

/// One sign-in flow at a time over the injected browser and capture
/// channel. The session holds the in-flight pair only; each flow's client
/// arrives with its begin (Microsoft and Google share the one channel and
/// the one slot), and it owns no token storage (that is the credential
/// transaction's half, over the OS store).
pub(crate) struct SignInSession {
    /// The system browser the authorization URL opens in.
    browser: Arc<dyn SystemBrowser>,
    /// The OAuth callbacks the deep-link capture forwards; the receiving
    /// end is held under an async lock so the in-flight flow's await is
    /// the only reader.
    callbacks: tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>,
    /// The sending half of the callback channel, held so a displaced
    /// waiter can requeue a redirect that belongs to a later flow.
    callback_tx: mpsc::UnboundedSender<String>,
    /// Which flow the in-flight pair belongs to, minting one id per
    /// begin so a displaced waiter can recognise itself.
    flow_seq: std::sync::atomic::AtomicU64,
    /// The one in-flight flow, if any.
    in_flight: Mutex<Option<InFlight>>,
}

impl SignInSession {
    /// A session that opens URLs in `browser` and reads redirects from
    /// `callbacks`.
    #[must_use]
    pub(crate) fn new(
        browser: Arc<dyn SystemBrowser>,
        callbacks: mpsc::UnboundedReceiver<String>,
        callback_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            browser,
            callbacks: tokio::sync::Mutex::new(callbacks),
            flow_seq: std::sync::atomic::AtomicU64::new(0),
            callback_tx,
            in_flight: Mutex::new(None),
        }
    }

    /// Begins one sign-in: mints the request over `client` and opens the
    /// authorization URL in the system browser, returning once the browser
    /// opened, so the caller can answer "browser opened" honestly before
    /// any wait. The redirect wait is [`SignInSession::wait_for_callback`]
    /// and the spend is [`SignInSession::complete_sign_in`].
    ///
    /// One sign-in at a time: while a flow stands (begun, not yet
    /// completed), a second begin answers [`SignInError::InFlight`]
    /// without opening anything. A browser that refuses to open leaves
    /// nothing in flight.
    ///
    /// # Errors
    ///
    /// Returns [`SignInError::InFlight`] when a sign-in already stands,
    /// and [`SignInError::Browser`] when the URL could not be opened.
    pub(crate) fn begin_sign_in(
        &self,
        client: &Arc<OAuthClient>,
        login_hint: Option<&str>,
    ) -> Result<u64, SignInError> {
        // Reserve the single slot before anything else: the refusal a
        // second begin meets is the flow that already stands, not a race
        // that both flowed through.
        let flow = self
            .flow_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let request = {
            let mut in_flight = self.in_flight.lock().expect("the in-flight lock stands");
            if in_flight.is_some() {
                return Err(SignInError::InFlight);
            }
            let request = client.begin(login_hint);
            *in_flight = Some(InFlight {
                flow,
                client: Arc::clone(client),
                state: request.state.clone(),
                verifier: request.pkce.verifier().to_owned(),
            });
            request
        };
        if let Err(err) = self.browser.open(&request.authorization_url) {
            // The flow never started: clear the slot so a retry is a
            // fresh begin rather than a permanent refusal.
            self.in_flight
                .lock()
                .expect("the in-flight lock stands")
                .take();
            return Err(SignInError::Browser(err));
        }
        Ok(flow)
    }

    /// Waits for the deep-link redirect of the flow [`begin_sign_in`]
    /// minted, handing its raw URL back whole for
    /// [`SignInSession::complete_sign_in`]. The wait ends when the capture
    /// forwards a URL or its channel closes.
    ///
    /// # Errors
    ///
    /// Returns [`SignInError::CaptureClosed`] when the capture went away
    /// before any redirect arrived.
    pub(crate) async fn wait_for_callback(&self, flow: u64) -> Result<String, SignInError> {
        let url = self
            .callbacks
            .lock()
            .await
            .recv()
            .await
            .ok_or(SignInError::CaptureClosed)?;
        // The flow this waiter was minted by may have been cancelled
        // while it held the channel: a redirect that arrives now belongs
        // to whichever flow stands, so hand it back and answer the
        // cancellation instead of completing a stranger's flow.
        let standing = self
            .in_flight
            .lock()
            .expect("the in-flight lock stands")
            .as_ref()
            .map(|in_flight| in_flight.flow);
        if standing != Some(flow) {
            let _ = self.callback_tx.send(url);
            return Err(SignInError::Cancelled);
        }
        Ok(url)
    }

    /// Completes the in-flight flow from the raw redirect URL: spends
    /// the held client, state, and verifier on the core's complete, where
    /// the `state` CSRF check stands and the code is exchanged. The flow
    /// is spent whether or not the completion succeeds; a refused callback
    /// (a mismatched state, a forged `iss`) ends the flow, and the next
    /// begin starts fresh.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Callback`] when no sign-in is in flight or
    /// the redirect is malformed or forged, and the core's own errors
    /// for the exchange beyond it.
    pub(crate) async fn complete_sign_in(
        &self,
        raw_url: &str,
        now: OffsetDateTime,
    ) -> Result<TokenSet, OAuthError> {
        // Spend the flow first, whatever the completion answers: the
        // state and verifier are one-shot, and a refused callback ends
        // the flow so the next begin starts fresh.
        let in_flight = self
            .in_flight
            .lock()
            .expect("the in-flight lock stands")
            .take()
            .ok_or_else(|| {
                OAuthError::Callback("no sign-in is in flight to complete".to_owned())
            })?;
        in_flight
            .client
            .complete(raw_url, &in_flight.state, &in_flight.verifier, now)
            .await
    }

    /// Takes the in-flight pair back: the way out for a sign-in the
    /// caller abandoned (the wizard closed while the redirect was still
    /// pending), so a later begin does not answer InFlight until the
    /// app restarts.
    pub(crate) fn cancel_sign_in(&self) -> bool {
        self.in_flight
            .lock()
            .expect("oauth in-flight lock poisoned")
            .take()
            .is_some()
    }

    /// Stands a pair in the in-flight slot without opening a browser:
    /// the test seam for the refusal and cancellation windows, where
    /// the product path mints its own pair through begin.
    #[cfg(test)]
    pub(crate) fn seed_in_flight_for_test(
        &self,
        client: &Arc<OAuthClient>,
        pair: (String, String),
    ) -> bool {
        self.in_flight
            .lock()
            .expect("oauth in-flight lock poisoned")
            .replace(InFlight {
                flow: self
                    .flow_seq
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1,
                client: Arc::clone(client),
                state: pair.0,
                verifier: pair.1,
            })
            .is_none()
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
