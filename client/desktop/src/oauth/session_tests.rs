// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sign-in session: the capture's filter (only the OAuth
//! scheme's URL, handed on whole), the single-flight rule (a second
//! begin while one stands refuses honestly, opening nothing), the raw
//! redirect's hand-back (byte for byte what the capture forwarded), the
//! browser seam (opened exactly once per flow), and the completion's
//! routing through the core's parser (the forged state refused, the
//! provider's own error carried through, no flow to spend refused). No
//! test touches a network: the client's exchange is only ever reached
//! past a parse the tests refuse. The async bodies run on one runtime
//! per test, the host's drive pattern (this crate forbids
//! `#[tokio::test]`'s `block_on` by clippy config).

use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use tokio::sync::mpsc;

use astra_oauth::{OAuthClient, OAuthError, OAuthProviderConfig};

use super::{
    oauth_callback, SignInError, SignInSession, SystemBrowser, OAUTH_REDIRECT_URI, OAUTH_SCHEME,
};

/// A browser stand-in that counts its opens and can be told to refuse:
/// the seam's contract (one open per begun flow, none on a refusal)
/// without any platform shell.
struct CountingBrowser {
    opens: Mutex<usize>,
    refuse: bool,
}

impl CountingBrowser {
    /// A stand-in that opens.
    fn opening() -> Arc<Self> {
        Arc::new(Self {
            opens: Mutex::new(0),
            refuse: false,
        })
    }

    /// A stand-in that answers every open with a refusal.
    fn refusing() -> Arc<Self> {
        Arc::new(Self {
            opens: Mutex::new(0),
            refuse: true,
        })
    }

    /// The opens that landed.
    fn opens(&self) -> usize {
        *self.opens.lock().unwrap()
    }
}

impl SystemBrowser for CountingBrowser {
    fn open(&self, _url: &str) -> Result<(), String> {
        let mut opens = self.opens.lock().unwrap();
        *opens += 1;
        if self.refuse {
            Err("the shell has no browser".to_owned())
        } else {
            Ok(())
        }
    }
}

/// A client over the redirect URI the scheme registers: minting only in
/// these tests, never an exchange (every completion is refused at the
/// parse).
fn client() -> Arc<OAuthClient> {
    let provider = OAuthProviderConfig::microsoft(
        "00000000-0000-0000-0000-000000000000",
        "common",
        OAUTH_REDIRECT_URI,
        &["offline_access"],
    );
    Arc::new(OAuthClient::new(provider).expect("the test client builds"))
}

/// A callback delivery on the OAuth scheme, raw as the OS hands it over.
const RAW_CALLBACK: &str = "com.kenizhou.astra://oauth?code=abc.def-ghi&state=xyz123";

/// The browser stand-in as the session holds it: the explicit cast,
/// because a nested `Arc::clone` argument does not unsize on its own.
fn as_seam(browser: &Arc<CountingBrowser>) -> Arc<dyn SystemBrowser> {
    Arc::clone(browser) as Arc<dyn SystemBrowser>
}

/// Feeds one raw callback into a fresh capture channel, handing back the
/// session's receiving end.
fn fed_capture() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(RAW_CALLBACK.to_owned())
        .expect("the fed delivery lands");
    rx
}

#[test]
fn the_capture_keeps_only_the_oauth_scheme_url_whole() {
    let urls: Vec<url::Url> = ["mailto:boss@example.test", RAW_CALLBACK]
        .iter()
        .map(|raw| raw.parse::<url::Url>())
        .collect::<Result<_, _>>()
        .expect("the delivery's urls parse");
    assert_eq!(
        oauth_callback(&urls).as_deref(),
        Some(RAW_CALLBACK),
        "the oauth scheme's url is handed on whole, byte for byte"
    );
}

#[test]
fn a_delivery_without_the_oauth_scheme_yields_nothing() {
    let urls: Vec<url::Url> = vec!["mailto:boss@example.test".parse().expect("the url parses")];
    assert_eq!(
        oauth_callback(&urls),
        None,
        "a mailto delivery never reaches a sign-in flow"
    );
}

#[test]
fn the_config_registers_the_oauth_callback_scheme() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json");
    let config = std::fs::read_to_string(path).expect("tauri.conf.json reads");
    let config: serde_json::Value = serde_json::from_str(&config).expect("tauri.conf.json parses");
    let schemes: Vec<String> = config
        .pointer("/plugins/deep-link/desktop/schemes")
        .and_then(serde_json::Value::as_array)
        .expect("the deep-link block names its desktop schemes")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();
    assert!(
        schemes.iter().any(|scheme| scheme == OAUTH_SCHEME),
        "the oauth callback scheme {OAUTH_SCHEME} must be registered for the OS to deliver it; the block names {schemes:?}"
    );
}

#[test]
fn a_begin_opens_the_browser_once_and_hands_the_raw_url_back_whole() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let session = SignInSession::new(client(), as_seam(&browser), fed_capture());

            let handed_back = session
                .begin_sign_in(None)
                .await
                .expect("the fed delivery completes the begin");

            assert_eq!(
                handed_back, RAW_CALLBACK,
                "the raw redirect crosses whole, never parsed by the host"
            );
            assert_eq!(browser.opens(), 1, "the browser opens exactly once");
        });
}

#[test]
fn a_second_begin_refuses_while_one_is_in_flight() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let session = SignInSession::new(client(), as_seam(&browser), fed_capture());

            // The first begin resolves once the fed delivery arrives, but
            // the flow it began stands until its completion spends it:
            // exactly the window in which a second begin must refuse.
            let first = session
                .begin_sign_in(None)
                .await
                .expect("the fed delivery completes the begin");
            assert_eq!(first, RAW_CALLBACK);

            let refused = session
                .begin_sign_in(None)
                .await
                .expect_err("a second begin while one stands refuses");
            assert!(
                matches!(refused, SignInError::InFlight),
                "the refusal names the in-flight flow, not the browser: {refused}"
            );
            assert_eq!(
                browser.opens(),
                1,
                "the refused begin opens nothing; the first flow's open stands alone"
            );
        });
}

#[test]
fn a_failed_browser_open_leaves_nothing_in_flight() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::refusing();
            let (_tx, rx) = mpsc::unbounded_channel::<String>();
            let session = SignInSession::new(client(), as_seam(&browser), rx);

            let refused = session
                .begin_sign_in(None)
                .await
                .expect_err("a browser that refuses ends the begin");
            assert!(
                matches!(refused, SignInError::Browser(_)),
                "the browser's refusal is the answer: {refused}"
            );

            // The failed open cleared the slot, so a retry is a fresh
            // begin (it reaches the browser again, refusing the same
            // honest way) rather than an in-flight refusal.
            let retried = session
                .begin_sign_in(None)
                .await
                .expect_err("the refusing browser refuses the retry too");
            assert!(
                matches!(retried, SignInError::Browser(_)),
                "the retry begins fresh rather than meeting a stale in-flight: {retried}"
            );
        });
}

#[test]
fn a_closed_capture_ends_the_wait() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let (tx, rx) = mpsc::unbounded_channel::<String>();
            drop(tx);
            let session = SignInSession::new(client(), as_seam(&browser), rx);

            let ended = session
                .begin_sign_in(None)
                .await
                .expect_err("a capture with no sender never delivers");
            assert!(
                matches!(ended, SignInError::CaptureClosed),
                "the closed capture is named: {ended}"
            );
            assert_eq!(browser.opens(), 1, "the browser still opened for the flow");
        });
}

#[test]
fn completion_refuses_a_forged_state() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let session = SignInSession::new(client(), as_seam(&browser), fed_capture());
            let raw = session
                .begin_sign_in(None)
                .await
                .expect("the fed delivery completes the begin");

            // A second URL, whose state differs from both the fed
            // delivery's and the minted one: the completion must spend
            // the minted state and refuse the forgery in the core's
            // parser.
            let forged = "com.kenizhou.astra://oauth?code=abc&state=someone-elses-state";
            assert_ne!(forged, raw, "the fixtures must differ");
            match session
                .complete_sign_in(forged, OffsetDateTime::UNIX_EPOCH)
                .await
            {
                Err(OAuthError::Callback(sentence)) => assert!(
                    sentence.contains("state mismatch"),
                    "the forged state is refused as a callback fault: {sentence}"
                ),
                other => panic!("a forged state must be refused, got {other:?}"),
            }
        });
}

#[test]
fn completion_carries_a_provider_refusal_through_the_parser() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let session = SignInSession::new(client(), as_seam(&browser), fed_capture());
            session
                .begin_sign_in(None)
                .await
                .expect("the fed delivery completes the begin");

            // The parser reads a provider error before any state, so the
            // refused consent crosses without the flow's own state at
            // hand.
            let refused = "com.kenizhou.astra://oauth?error=access_denied&error_description=nope";
            match session
                .complete_sign_in(refused, OffsetDateTime::UNIX_EPOCH)
                .await
            {
                Err(OAuthError::Endpoint { error, .. }) => assert_eq!(
                    error, "access_denied",
                    "the provider's refusal code crosses whole"
                ),
                other => panic!("a provider refusal must be carried, got {other:?}"),
            }
        });
}

#[test]
fn completion_without_a_sign_in_refuses() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let (_tx, rx) = mpsc::unbounded_channel::<String>();
            let session = SignInSession::new(client(), as_seam(&browser), rx);

            match session
                .complete_sign_in(RAW_CALLBACK, OffsetDateTime::UNIX_EPOCH)
                .await
            {
                Err(OAuthError::Callback(sentence)) => assert!(
                    sentence.contains("no sign-in is in flight"),
                    "the missing flow is named: {sentence}"
                ),
                other => panic!("a completion with nothing in flight must refuse, got {other:?}"),
            }
        });
}

#[test]
fn cancel_takes_the_in_flight_pair_back_so_a_later_begin_starts_fresh() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            // A capture nobody feeds: the begin below resolves on its own
            // delivered-pair seam, leaving the flow in flight, exactly the
            // abandoned wizard's shape.
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let session = SignInSession::new(client(), as_seam(&browser), rx);

            let seeded =
                session.seed_in_flight_for_test(("state-1".to_owned(), "verifier-1".to_owned()));
            assert!(seeded, "the pair stands in flight");
            assert!(session.cancel_sign_in());
            let seeded_again =
                session.seed_in_flight_for_test(("state-2".to_owned(), "verifier-2".to_owned()));
            assert!(seeded_again, "a later begin starts fresh, not InFlight");
        });
}
