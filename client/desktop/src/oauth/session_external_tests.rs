// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the session's external-flow arm: a sign-in the JMAP leg
//! discovers outside the session (its authorization URL already minted
//! by `astra-account`'s standards discovery) still runs through the one
//! session, because the redirect lands on the same deep-link channel
//! every flow shares. The session opens the given URL, holds the one
//! in-flight slot exactly a minted flow does, and the caller's own
//! completion spends the slot through the claim.

use std::sync::Arc;

use tokio::sync::mpsc;

use astra_oauth::OAuthError;

use super::tests::{as_seam, CountingBrowser};
use super::{SignInError, SignInSession};

/// The URL a JMAP flow's discovery minted: an authorization page on some
/// server the session knows nothing about beyond this string.
const DISCOVERED_URL: &str = "https://mail.example.net/oauth/authorize?client_id=client-xyz";

/// A callback delivery on the OAuth scheme, raw as the OS hands it over.
const RAW_CALLBACK: &str = "com.kenizhou.astra://oauth?code=abc.def-ghi&state=xyz123";

/// A session over the opening stand-in and a fed capture channel.
fn fed_session() -> (Arc<SignInSession>, Arc<CountingBrowser>) {
    let browser = CountingBrowser::opening();
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(RAW_CALLBACK.to_owned());
    (
        Arc::new(SignInSession::new(
            as_seam(&browser),
            rx,
            mpsc::unbounded_channel().0,
        )),
        browser,
    )
}

#[test]
fn an_external_flow_opens_the_given_url_and_holds_the_slot() {
    let (session, browser) = fed_session();

    session
        .begin_external_sign_in(DISCOVERED_URL)
        .expect("the browser opened the discovered url");

    assert_eq!(browser.opens(), 1, "the given url opened, once");
    assert!(
        matches!(
            session.begin_external_sign_in(DISCOVERED_URL),
            Err(SignInError::InFlight)
        ),
        "an external flow refuses a second begin exactly a minted one does"
    );
}

#[test]
fn an_external_flow_whose_browser_refused_leaves_nothing_in_flight() {
    let browser = CountingBrowser::refusing();
    let (tx, rx) = mpsc::unbounded_channel();
    let session = SignInSession::new(as_seam(&browser), rx, tx);

    assert!(
        matches!(
            session.begin_external_sign_in(DISCOVERED_URL),
            Err(SignInError::Browser(_))
        ),
        "the refusing browser fails the begin"
    );
    // The slot is free: a retry meets the browser's refusal again, never
    // an in-flight one.
    assert!(
        matches!(
            session.begin_external_sign_in(DISCOVERED_URL),
            Err(SignInError::Browser(_))
        ),
        "the retry meets the browser's refusal, not an in-flight one"
    );
}

#[test]
fn the_wait_hands_the_redirect_back_and_the_claim_spends_the_slot() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let (session, _browser) = fed_session();

    let flow = session
        .begin_external_sign_in(DISCOVERED_URL)
        .expect("the flow begins");
    let raw = runtime.block_on(|| async { session.wait_for_callback(flow).await });
    assert_eq!(
        raw.expect("the fed delivery arrives"),
        RAW_CALLBACK,
        "the redirect crosses whole, byte for byte"
    );

    session
        .claim_external_sign_in()
        .expect("the claim spends the standing external flow");
    // The slot is spent: a second claim meets the completion's own
    // nothing-stands refusal, and a fresh begin starts clean.
    assert!(
        matches!(
            session.claim_external_sign_in(),
            Err(OAuthError::Callback(_))
        ),
        "a second claim meets the spent-slot refusal"
    );
    assert!(
        session.begin_external_sign_in(DISCOVERED_URL).is_ok(),
        "the next flow begins fresh"
    );
}

#[test]
fn a_cancelled_external_flow_answers_the_wait_with_the_cancellation() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CountingBrowser::opening();
    let (tx, rx) = mpsc::unbounded_channel();
    let session = SignInSession::new(as_seam(&browser), rx, mpsc::unbounded_channel().0);

    let outcome = runtime.block_on(|| async {
        let flow = session
            .begin_external_sign_in(DISCOVERED_URL)
            .expect("the flow begins");
        // The wait is lazy: the cancel lands before its first poll, and
        // the delivery that arrives meanwhile belongs to no standing
        // flow, so the displaced waiter hands it back and answers the
        // cancellation.
        let wait = session.wait_for_callback(flow);
        assert!(session.cancel_sign_in(), "the flow stood to be cancelled");
        tx.send(RAW_CALLBACK.to_owned())
            .expect("the delivery lands");
        wait.await
    });
    assert!(
        matches!(outcome, Err(SignInError::Cancelled)),
        "the displaced waiter answers the cancellation: {outcome:?}"
    );
}
