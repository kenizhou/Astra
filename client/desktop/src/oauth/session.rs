// SPDX-License-Identifier: GPL-3.0-only
//! The system-browser sign-in session: the host half of one
//! Authorization-Code-with-PKCE flow. The session mints nothing itself;
//! [`OAuthClient::begin`] builds the URL, the state, and the PKCE pair,
//! and the session's job is the platform half around them: open the URL
//! in the system browser (never an embedded webview, per the design's
//! D2), wait for the redirect the browser lands on, and hand that raw
//! URL back for [`OAuthClient::complete`], where the `state` CSRF check
//! stands.
//!
//! The redirect arrives on one of two captures, and the begin names
//! which: Microsoft's flow lands on the registered custom scheme, whose
//! deliveries the deep-link plugin forwards into the session's one
//! shared channel; a Google Desktop flow lands on the loopback port
//! that flow's own listener bound ([`super::loopback`]), whose single
//! capture crosses on its own channel. The two never mix: a deep-link
//! delivery does not feed a loopback flow, and a loopback capture
//! belongs to the flow whose port it landed on.
//!
//! A third shape of flow runs through the same session: an **external**
//! one, whose authorization URL was minted outside it (the JMAP leg's
//! standards discovery, which registers its own client and builds its
//! own PKCE pair in `astra-account`). The session treats it exactly as
//! a minted flow for everything platform-side: it opens the given URL,
//! holds the one in-flight slot, and waits on the shared capture; only
//! the completion differs, which is why the spend is the caller's own
//! through [`SignInSession::claim_external_sign_in`].
//!
//! The two platform seams are injectable so the whole session is driven
//! in tests without a Tauri runtime: [`SystemBrowser`] (the shell's
//! opener in production, a counting stand-in in tests) and the callback
//! channel (the receiving end of the deep-link capture's forwarding; a
//! test feeds it directly). The capture's production shape, the
//! plugin's `on_open_url` event filtered by [`oauth_callback`], lives in
//! [`super::wiring`], and its behaviour is pinned there and by the
//! session tests.
//!
//! The client is a per-flow input, not session state: Microsoft and
//! Google flows share one session (one channel, one in-flight slot),
//! and each begin names the client that mints it (a Google flow's
//! client carries the port its listener bound).
//!
//! [`OAuthClient::begin`]: astra_oauth::OAuthClient::begin
//! [`OAuthClient::complete`]: astra_oauth::OAuthClient::complete
//! [`oauth_callback`]: super::wiring::oauth_callback

use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use tokio::sync::mpsc;

use astra_oauth::{OAuthClient, OAuthError, TokenSet};

use super::loopback::LoopbackCapture;

/// The custom scheme every OAuth callback redirect lands on: the app's
/// own identifier, the one `tauri.conf.json`'s deep-link block registers
/// beside `mailto`. The OS only delivers links on registered schemes, so
/// this is the contract between the provider's registered redirect URI
/// and the capture's filter.
pub(crate) const OAUTH_SCHEME: &str = "com.kenizhou.astra";

/// The redirect URI Microsoft's provider config registers: the OAuth
/// path on this app's scheme, the shape `astra-oauth`'s callback parser
/// names. A Google Desktop flow redirects to the loopback port its own
/// listener bound instead ([`super::loopback`]); the JMAP leg's
/// discovered registration names this one, because a dynamically
/// registered client on a personal machine has no loopback listener of
/// its own.
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
    /// The loopback capture ended without a redirect: its bound passed
    /// or its socket failed, and the sentence names which.
    #[error("the loopback capture failed: {0}")]
    LoopbackFailed(String),
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

/// Which capture a flow's redirect arrives on: the deep-link channel
/// Microsoft's registered scheme feeds, or a Google Desktop flow's own
/// loopback listener, handed over with its receiver.
pub(crate) enum CaptureSource {
    /// The deep-link plugin's forwarded callbacks: the session's one
    /// shared channel, whose deliveries belong to whichever flow stands.
    DeepLink,
    /// The flow's loopback listener, whose one capture crosses on its
    /// own receiver.
    Loopback(mpsc::Receiver<LoopbackCapture>),
}

/// What a standing flow spends its completion on: the client the
/// session minted it over, or nothing, when the flow was minted
/// outside the session and its completion belongs to the caller.
enum FlowKind {
    /// A flow the session's own client minted: the completion spends
    /// the client, the state, and the verifier on the core's complete.
    Minted {
        /// The client whose flow this is; the completion's exchange
        /// runs on it, so the flow spends the client it was minted by.
        client: Arc<OAuthClient>,
        /// The CSRF state issued at begin; the callback must echo it.
        state: String,
        /// The PKCE verifier paired with the challenge the
        /// authorization request carried.
        verifier: String,
    },
    /// A flow minted outside the session (the JMAP leg's discovered
    /// request): the session opened its URL and holds its slot, and
    /// the caller's own completion spends it through
    /// [`SignInSession::claim_external_sign_in`].
    External,
}

/// One in-flight flow's held half: what it was minted by and the
/// capture the redirect arrives on. Held from its begin until its
/// completion spent it, so a second begin can be refused honestly.
struct InFlight {
    /// Which flow this is: minted from the session's counter, so a
    /// waiter abandoned by a cancel can tell its own redirect from the
    /// next flow's and hand the latter back rather than steal it.
    flow: u64,
    /// What the flow spends its completion on.
    kind: FlowKind,
    /// Whether this flow's redirect arrives on its own loopback
    /// listener rather than the shared deep-link channel.
    loopback: bool,
    /// The loopback capture's receiver, present from begin until this
    /// flow's waiter claims it; `None` on a deep-link flow and after
    /// the claim, so the in-flight lock never crosses an await.
    loopback_rx: Option<mpsc::Receiver<LoopbackCapture>>,
}

impl InFlight {
    /// Claims this flow's loopback capture for one wait, if it has one:
    /// `Ok(None)` on a deep-link flow, the receiver on a loopback flow's
    /// first wait, and the honest refusal on a second.
    fn claim_loopback(&mut self) -> Result<Option<mpsc::Receiver<LoopbackCapture>>, SignInError> {
        if !self.loopback {
            return Ok(None);
        }
        self.loopback_rx.take().map(Some).ok_or_else(|| {
            SignInError::LoopbackFailed(
                "the capture was already claimed by an earlier wait".to_owned(),
            )
        })
    }
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
    /// any wait. `capture` names where this flow's redirect arrives (the
    /// shared deep-link channel, or a loopback flow's own listener); the
    /// loopback listener must already be bound, because the client it
    /// minted carries the port in its redirect. The redirect wait is
    /// [`SignInSession::wait_for_callback`] and the spend is
    /// [`SignInSession::complete_sign_in`].
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
        capture: CaptureSource,
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
            let (loopback, loopback_rx) = match capture {
                CaptureSource::DeepLink => (false, None),
                CaptureSource::Loopback(receiver) => (true, Some(receiver)),
            };
            *in_flight = Some(InFlight {
                flow,
                kind: FlowKind::Minted {
                    client: Arc::clone(client),
                    state: request.state.clone(),
                    verifier: request.pkce.verifier().to_owned(),
                },
                loopback,
                loopback_rx,
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

    /// Waits for the redirect of the flow [`begin_sign_in`] minted, on
    /// the capture that flow named, handing its raw URL back whole for
    /// [`SignInSession::complete_sign_in`]. The wait ends when the
    /// capture forwards a URL or its channel closes.
    ///
    /// # Errors
    ///
    /// Returns [`SignInError::CaptureClosed`] when the capture went away
    /// before any redirect arrived, and [`SignInError::LoopbackFailed`]
    /// when a loopback capture's bound passed or its socket failed.
    pub(crate) async fn wait_for_callback(&self, flow: u64) -> Result<String, SignInError> {
        // A loopback flow's capture arrives on its own listener, not the
        // deep-link channel: claim the receiver under the lock so the
        // mutex never crosses the await.
        let loopback = self
            .in_flight
            .lock()
            .expect("the in-flight lock stands")
            .as_mut()
            .map(InFlight::claim_loopback)
            .transpose()?
            .flatten();
        if let Some(mut capture) = loopback {
            return match capture.recv().await {
                Some(LoopbackCapture::Received(url)) => {
                    // The cancelled-waiter check, as below, except a
                    // loopback capture is never requeued: each flow binds
                    // its own port, so a capture on this one belongs to
                    // no other flow.
                    if self.standing_flow() != Some(flow) {
                        return Err(SignInError::Cancelled);
                    }
                    Ok(url)
                }
                Some(LoopbackCapture::Failed(detail)) => Err(SignInError::LoopbackFailed(detail)),
                // Only a capture thread that died without its last word
                // ends the channel with nothing on it.
                None => Err(SignInError::LoopbackFailed(
                    "the capture ended without an answer".to_owned(),
                )),
            };
        }
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
        if self.standing_flow() != Some(flow) {
            let _ = self.callback_tx.send(url);
            return Err(SignInError::Cancelled);
        }
        Ok(url)
    }

    /// Which flow stands in the in-flight slot, if any.
    fn standing_flow(&self) -> Option<u64> {
        self.in_flight
            .lock()
            .expect("the in-flight lock stands")
            .as_ref()
            .map(|in_flight| in_flight.flow)
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
        match in_flight.kind {
            FlowKind::Minted {
                client,
                state,
                verifier,
            } => client.complete(raw_url, &state, &verifier, now).await,
            FlowKind::External => Err(OAuthError::Callback(
                "an externally minted flow completes through its own half".to_owned(),
            )),
        }
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

    /// Whether the standing flow's loopback receiver has been claimed by
    /// its waiter: the test probe for the claim-then-cancel ordering,
    /// where a cancel racing an unclaimed receiver would drop the
    /// capture channel the late send needs.
    #[cfg(test)]
    pub(crate) fn loopback_claimed(&self) -> bool {
        self.in_flight
            .lock()
            .expect("the in-flight lock stands")
            .as_ref()
            .is_some_and(|in_flight| in_flight.loopback_rx.is_none())
    }
    /// Stands a pair in the in-flight slot without opening a browser:
    /// the test seam for the refusal and cancellation windows, where
    /// the product path mints its own pair through begin. The seeded
    /// pair waits on the deep-link channel, the arm whose windows the
    /// seam drives.
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
                kind: FlowKind::Minted {
                    client: Arc::clone(client),
                    state: pair.0,
                    verifier: pair.1,
                },
                loopback: false,
                loopback_rx: None,
            })
            .is_none()
    }
}

mod external;

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "session_external_tests.rs"]
mod external_tests;
