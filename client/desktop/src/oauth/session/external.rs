// SPDX-License-Identifier: GPL-3.0-only
//! The session's external-flow arm, split from the parent for the file
//! cap: the begin that opens an already-minted authorization URL and the
//! claim its caller's completion spends. The single-slot and capture
//! machinery it runs on is the parent's; only the two entry points live
//! here, and they enforce the same one-flow-at-a-time rule the minted
//! arm does.

use super::{FlowKind, InFlight, SignInError, SignInSession};

impl SignInSession {
    /// Begins one externally minted sign-in: opens `authorization_url`
    /// (already carrying its own state and PKCE pair, minted by whoever
    /// discovered the flow) in the system browser and holds the single
    /// in-flight slot for it, exactly as the minted begin does. The
    /// redirect wait is the shared [`wait_for_callback`]; the spend is
    /// the caller's own, by way of [`claim_external_sign_in`].
    ///
    /// # Errors
    ///
    /// Returns [`SignInError::InFlight`] when a sign-in already stands,
    /// and [`SignInError::Browser`] when the URL could not be opened;
    /// each leaves nothing new in flight.
    ///
    /// [`wait_for_callback`]: SignInSession::wait_for_callback
    /// [`claim_external_sign_in`]: SignInSession::claim_external_sign_in
    pub(crate) fn begin_external_sign_in(
        &self,
        authorization_url: &str,
    ) -> Result<u64, SignInError> {
        let flow = self
            .flow_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        {
            let mut in_flight = self.in_flight.lock().expect("the in-flight lock stands");
            if in_flight.is_some() {
                return Err(SignInError::InFlight);
            }
            *in_flight = Some(InFlight {
                flow,
                kind: FlowKind::External,
                loopback: false,
                loopback_rx: None,
            });
        }
        if let Err(err) = self.browser.open(authorization_url) {
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

    /// Spends the standing external flow so its caller's own completion
    /// can run: the slot is taken first, exactly the minted completion
    /// takes its own, so the exchange that follows cannot be raced by a
    /// second begin and a refused completion still leaves the next
    /// begin fresh.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Callback`] when no external flow stands.
    pub(crate) fn claim_external_sign_in(&self) -> Result<(), astra_oauth::OAuthError> {
        let standing = self
            .in_flight
            .lock()
            .expect("the in-flight lock stands")
            .take()
            .ok_or_else(|| {
                astra_oauth::OAuthError::Callback("no sign-in is in flight to complete".to_owned())
            })?;
        match standing.kind {
            FlowKind::External => Ok(()),
            FlowKind::Minted { .. } => Err(astra_oauth::OAuthError::Callback(
                "the standing flow completes through the session's own completion".to_owned(),
            )),
        }
    }
}
