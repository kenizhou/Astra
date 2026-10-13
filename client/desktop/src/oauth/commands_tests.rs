// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the wizard's sign-in commands: the offer names exactly the
//! registered providers; begin opens the browser and answers "browser
//! opened" honestly (every refusal path leaves nothing in flight); the
//! completion runs on the runtime and its result crosses the emit seam
//! with the ported classification word; cancel takes the flow back. The
//! registrations are compile-time inputs, so the tests drive the seeded
//! seam (`with_clients`) rather than the environment, and no test
//! touches a network: the driven completion is the front-channel consent
//! refusal, which the core's parser refuses before any exchange, and
//! the success and grant-refusal arms are pinned through the pure
//! outcome mappings the completion calls.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use astra_oauth::{GrantRefusal, OAuthClient, OAuthError, OAuthProviderConfig, TokenSet};

use super::{
    begin_with, cancel_with, complete_outcome, wait_outcome, BrowserOpened, OAuthCommandError,
    OAuthOfferView, ProviderKind, RefusalKind, SignInOutcome, SignInResult, SignInSurface,
    SystemBrowser, OAUTH_REDIRECT_URI,
};

/// A browser stand-in that counts its opens and can be told to refuse:
/// the command pair's contract against the seam without any shell.
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

/// A client over the redirect URI the scheme registers: minting and
/// parsing only, never an exchange (the driven completion is refused at
/// the parse).
fn client() -> Arc<OAuthClient> {
    let provider = OAuthProviderConfig::microsoft(
        "00000000-0000-0000-0000-000000000000",
        "common",
        OAUTH_REDIRECT_URI,
        &["offline_access"],
    );
    Arc::new(OAuthClient::new(provider).expect("the test client builds"))
}

/// The browser stand-in as the surface holds it.
fn as_seam(browser: &Arc<CountingBrowser>) -> Arc<dyn SystemBrowser> {
    Arc::clone(browser) as Arc<dyn SystemBrowser>
}

/// A capture nobody feeds: the begun flow stands in flight until the
/// test cancels it or drops the surface.
fn quiet_capture() -> mpsc::UnboundedReceiver<String> {
    let (_tx, rx) = mpsc::unbounded_channel();
    rx
}

/// Feeds one raw callback into a fresh capture channel, handing back the
/// receiving end. The delivery is the front-channel consent refusal, so
/// the completion the runtime spawns refuses at the core's parser and
/// never reaches a network.
fn refused_consent_capture() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(
        "com.kenizhou.astra://oauth?error=access_denied&error_description=not%20today".to_owned(),
    )
    .expect("the fed delivery lands");
    rx
}

/// The surface over given clients and capture, on the counting browser.
fn surface(
    clients: Vec<(ProviderKind, Arc<OAuthClient>)>,
    browser: &Arc<CountingBrowser>,
    callbacks: mpsc::UnboundedReceiver<String>,
) -> Arc<SignInSurface> {
    Arc::new(SignInSurface::with_clients(
        as_seam(browser),
        callbacks,
        clients,
    ))
}

/// The emit seam as the tests hold it: one channel the result lands in.
/// The sender stays in the pair so a test that never receives keeps the
/// channel open for the runtime's task.
fn emit_seam() -> (
    std::sync::mpsc::Sender<SignInResult>,
    std::sync::mpsc::Receiver<SignInResult>,
    impl FnOnce(SignInResult) + Send + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let sender = tx.clone();
    let emit = move |result: SignInResult| {
        // The real emitter ignores a webview that has gone away; the
        // seam does the same for a test that ends first.
        let _ = sender.send(result);
    };
    (tx, rx, emit)
}

/// How long a test waits for the runtime's completion to reach the emit
/// seam: the driven path refuses at a parse, so anything slower is a
/// fault, and the bound keeps a broken run from hanging.
const EMIT_BOUND: Duration = Duration::from_secs(5);

/// The wire literals, byte for byte the JSON the web's oauth.test.ts
/// reads: the provider words cross bare and snake_cased, the success as
/// a bare word, the refusal as its tagged record.
#[test]
fn the_views_cross_as_the_web_reads_them() {
    assert_eq!(
        serde_json::to_value(OAuthOfferView {
            providers: vec![ProviderKind::Microsoft, ProviderKind::Google],
        })
        .expect("the offer serialises"),
        serde_json::json!({"providers": ["microsoft", "google"]})
    );
    assert_eq!(
        serde_json::to_value(BrowserOpened {
            provider: ProviderKind::Microsoft
        })
        .expect("the browser-opened answer serialises"),
        serde_json::json!({"provider": "microsoft"})
    );
    assert_eq!(
        serde_json::to_value(SignInResult {
            provider: ProviderKind::Microsoft,
            outcome: SignInOutcome::SignedIn,
        })
        .expect("the success serialises"),
        serde_json::json!({"provider": "microsoft", "outcome": "signed_in"})
    );
    assert_eq!(
        serde_json::to_value(SignInResult {
            provider: ProviderKind::Google,
            outcome: SignInOutcome::Refused {
                refusal: RefusalKind::Dead,
                detail: "oauth endpoint error: invalid_grant; (no description)".to_owned(),
            },
        })
        .expect("the refusal serialises"),
        serde_json::json!({"provider": "google", "outcome": {"refused": {"refusal": "dead", "detail": "oauth endpoint error: invalid_grant; (no description)"}}})
    );
}

#[test]
fn the_offer_names_exactly_the_registered_providers() {
    let unregistered = surface(Vec::new(), &CountingBrowser::opening(), quiet_capture());
    assert_eq!(
        unregistered.offer(),
        OAuthOfferView {
            providers: Vec::new()
        },
        "a build carrying no registration offers nothing"
    );

    let registered = surface(
        vec![
            (ProviderKind::Microsoft, client()),
            (ProviderKind::Google, client()),
        ],
        &CountingBrowser::opening(),
        quiet_capture(),
    );
    assert_eq!(
        registered.offer().providers,
        vec![ProviderKind::Microsoft, ProviderKind::Google],
        "the offer is exactly the registered providers"
    );
}

#[test]
fn a_begun_sign_in_answers_browser_opened_and_the_refused_consent_crosses_whole() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CountingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Microsoft, client())],
        &browser,
        refused_consent_capture(),
    );
    let (_tx, results, emit) = emit_seam();

    let opened = begin_with(&desk, &runtime, emit, ProviderKind::Microsoft, None)
        .expect("the browser opened");
    assert_eq!(
        opened,
        BrowserOpened {
            provider: ProviderKind::Microsoft
        },
        "begin answers the browser-opened state for the provider it opened"
    );
    assert_eq!(browser.opens(), 1, "the system browser opened exactly once");

    let result = results
        .recv_timeout(EMIT_BOUND)
        .expect("the completion reaches the emit seam");
    assert_eq!(result.provider, ProviderKind::Microsoft);
    match result.outcome {
        SignInOutcome::Refused { refusal, detail } => {
            // A withdrawn consent is neither of the two grant-refusal
            // codes, so the ported classification reads it exactly as
            // anything unlearned: indeterminate, with the provider's own
            // code carried in the detail.
            assert_eq!(refusal, RefusalKind::Indeterminate);
            assert!(
                detail.contains("access_denied"),
                "the provider's own refusal code rides in the detail: {detail}"
            );
        }
        other @ SignInOutcome::SignedIn => panic!("a withdrawn consent must refuse, got {other:?}"),
    }
}

#[test]
fn the_outcome_mappings_follow_the_ported_classification() {
    let token_set = TokenSet {
        access_token: astra_oauth::Secret::new("token".to_owned()),
        refresh_token: None,
        expires_at: time::OffsetDateTime::UNIX_EPOCH,
        scope: String::new(),
        token_type: "Bearer".to_owned(),
    };
    assert_eq!(
        complete_outcome(Ok(token_set)),
        SignInOutcome::SignedIn,
        "a token set the provider issued is the success the wizard words"
    );

    let endpoint = |error: &str| OAuthError::Endpoint {
        error: error.to_owned(),
        description: None,
    };
    let dead = complete_outcome(Err(endpoint("invalid_grant")));
    let underscoped = complete_outcome(Err(endpoint("invalid_scope")));
    let indeterminate = complete_outcome(Err(OAuthError::Decode("a fault".to_owned())));
    assert_eq!(
        dead,
        SignInOutcome::Refused {
            refusal: RefusalKind::Dead,
            detail: "oauth endpoint error: invalid_grant; (no description)".to_owned(),
        },
        "an expired or revoked grant reads dead"
    );
    assert_eq!(
        underscoped,
        SignInOutcome::Refused {
            refusal: RefusalKind::Underscoped,
            detail: "oauth endpoint error: invalid_scope; (no description)".to_owned(),
        },
        "a grant issued narrow reads underscoped"
    );
    assert_eq!(
        indeterminate,
        SignInOutcome::Refused {
            refusal: RefusalKind::Indeterminate,
            detail: "decoding oauth response: a fault".to_owned(),
        },
        "anything the two codes do not name reads indeterminate"
    );
    assert_eq!(
        RefusalKind::from(GrantRefusal::Dead),
        RefusalKind::Dead,
        "the wire word mirrors the core's own classification"
    );
    assert_eq!(
        RefusalKind::from(GrantRefusal::Underscoped),
        RefusalKind::Underscoped
    );
    assert_eq!(
        RefusalKind::from(GrantRefusal::Indeterminate),
        RefusalKind::Indeterminate
    );

    let closed = wait_outcome(&super::SignInError::CaptureClosed);
    assert_eq!(
        closed,
        SignInOutcome::Refused {
            refusal: RefusalKind::Indeterminate,
            detail: "the deep-link capture closed before the redirect arrived".to_owned(),
        },
        "a capture that closed learned nothing about the grant"
    );
}

#[test]
fn begin_refuses_a_provider_this_build_does_not_offer() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CountingBrowser::opening();
    let desk = surface(Vec::new(), &browser, quiet_capture());
    let (_tx, _results, emit) = emit_seam();

    let refused = begin_with(&desk, &runtime, emit, ProviderKind::Google, None)
        .expect_err("a provider without a registration is not offered");
    assert!(
        matches!(refused, OAuthCommandError::NotOffered(ProviderKind::Google)),
        "the refusal names the unregistered provider: {refused}"
    );
    assert_eq!(browser.opens(), 0, "an unregistered provider opens nothing");
}

#[test]
fn a_second_begin_refuses_while_one_signin_stands() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CountingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Microsoft, client())],
        &browser,
        quiet_capture(),
    );
    let (_tx, _results, emit) = emit_seam();
    begin_with(
        &desk,
        &runtime,
        emit,
        ProviderKind::Microsoft,
        Some("work@example.test"),
    )
    .expect("the browser opened for the first flow");

    let (_tx, _results, second_emit) = emit_seam();
    let refused = begin_with(&desk, &runtime, second_emit, ProviderKind::Microsoft, None)
        .expect_err("one sign-in at a time");
    assert!(
        matches!(refused, OAuthCommandError::InFlight),
        "the refusal names the standing flow: {refused}"
    );
    assert_eq!(browser.opens(), 1, "the refused begin opens nothing");

    // The standing flow is spent by its cancel, so the wait the first
    // begin spawned ends silently over the quiet capture.
    assert!(cancel_with(&desk), "the standing flow cancels");
}

#[test]
fn a_browser_that_refuses_answers_the_error_and_leaves_a_fresh_retry() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CountingBrowser::refusing();
    let desk = surface(
        vec![(ProviderKind::Microsoft, client())],
        &browser,
        quiet_capture(),
    );
    let (_tx, _results, emit) = emit_seam();

    let refused = begin_with(&desk, &runtime, emit, ProviderKind::Microsoft, None)
        .expect_err("a browser that refuses ends the begin");
    assert!(
        matches!(refused, OAuthCommandError::Browser(_)),
        "the browser's refusal is the answer: {refused}"
    );

    let (_tx, _results, retry_emit) = emit_seam();
    let retried = begin_with(&desk, &runtime, retry_emit, ProviderKind::Microsoft, None)
        .expect_err("the refusing browser refuses the retry too");
    assert!(
        matches!(retried, OAuthCommandError::Browser(_)),
        "the retry begins fresh rather than meeting a stale in-flight: {retried}"
    );
    assert_eq!(browser.opens(), 2, "each begin reached the browser once");
}

#[test]
fn cancel_takes_the_flow_back_and_a_later_begin_starts_fresh() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CountingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Microsoft, client())],
        &browser,
        quiet_capture(),
    );
    let (_tx, _results, emit) = emit_seam();
    begin_with(&desk, &runtime, emit, ProviderKind::Microsoft, None)
        .expect("the browser opened for the flow");

    assert!(cancel_with(&desk), "the abandoned flow cancels");
    let (_tx, _results, again_emit) = emit_seam();
    begin_with(&desk, &runtime, again_emit, ProviderKind::Microsoft, None)
        .expect("a later begin starts fresh after the cancel");
    assert_eq!(browser.opens(), 2, "the fresh begin opened the browser");

    assert!(cancel_with(&desk), "the fresh flow cancels too");
    assert!(
        !cancel_with(&desk),
        "a cancel with nothing standing says so"
    );
}
