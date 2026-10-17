// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the JMAP sign-in command surface: the offer pull is bounded
//! and fails soft (a probe that never answers, or a server that
//! advertises nothing, crosses as a plain false, never an error), begin
//! runs the discovered authorization URL through the one session's
//! external arm (opening the system browser, refusing honestly while a
//! flow stands, leaving nothing in flight when the discovery or the
//! browser refuses), and the spawned half spends the redirect on the
//! caller's completion, emitting the result with the ported
//! classification word. No test touches a network: the begin seam
//! answers a minted start, and the completion is the caller's own.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use astra_account::{AccountError, JmapLoginStart};

use crate::oauth::commands::OAuthCommandError;

use super::{
    jmap_begin_with, jmap_offered_with, JmapBegin, JmapCompletion, JmapFlowHalves,
    JmapSignInOutcome, JmapSignInResult, SignInSurface,
};

/// The start a scripted discovery answers: an authorization URL and an
/// opaque pending handle, exactly what `astra-account`'s begin mints.
fn discovered_start() -> JmapLoginStart {
    JmapLoginStart {
        authorization_url: "https://mail.example.net/oauth/authorize?client_id=client-xyz"
            .to_owned(),
        pending: r#"{"email":"alice@example.test"}"#.to_owned(),
    }
}

/// A callback delivery on the OAuth scheme, raw as the OS hands it over.
const RAW_CALLBACK: &str = "com.kenizhou.astra://oauth?code=abc.def-ghi&state=xyz123";

/// A browser stand-in that records every URL it opened.
struct RecordingBrowser {
    urls: Mutex<Vec<String>>,
}

impl RecordingBrowser {
    /// A stand-in that opens.
    fn opening() -> Arc<Self> {
        Arc::new(Self {
            urls: Mutex::new(Vec::new()),
        })
    }

    /// The urls that opened.
    fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

impl crate::oauth::session::SystemBrowser for RecordingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.urls.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

/// A capture nobody feeds: the begun flow stands until the test ends.
fn quiet_capture() -> mpsc::UnboundedReceiver<String> {
    let (_tx, rx) = mpsc::unbounded_channel();
    rx
}

/// Feeds one raw callback into a fresh capture channel, handing back the
/// receiving end.
fn fed_capture() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(RAW_CALLBACK.to_owned())
        .expect("the fed delivery lands");
    rx
}

/// The surface over a browser stand-in and a capture, carrying no
/// registered client (the JMAP flow needs none).
fn surface(
    browser: Arc<RecordingBrowser>,
    callbacks: mpsc::UnboundedReceiver<String>,
) -> Arc<SignInSurface> {
    let tx = mpsc::unbounded_channel().0;
    Arc::new(SignInSurface::with_clients(
        browser,
        callbacks,
        tx,
        Vec::new(),
    ))
}

/// The begin seam answering the discovered start.
fn discovered_begin() -> JmapBegin {
    Arc::new(|_email, _server_url| Box::pin(async move { Ok(discovered_start()) }))
}

/// The begin seam that never answers.
fn hanging_begin() -> JmapBegin {
    Arc::new(|_email, _server_url| {
        Box::pin(async move {
            let () = std::future::pending().await;
            unreachable!("the hanging begin never answers")
        })
    })
}

/// The completion seam answering one outcome for any redirect.
fn completion_with(outcome: JmapSignInOutcome) -> JmapCompletion {
    Box::new(move |_pending, _callback| Box::pin(async move { outcome }))
}

/// How long a bounded test waits: enough for the runtime's spawn, small
/// enough that a hanging probe fails fast.
const SMALL_BOUND: Duration = Duration::from_millis(200);

/// The halves over one begin seam and one scripted outcome.
fn halves(begin: JmapBegin, outcome: JmapSignInOutcome) -> JmapFlowHalves {
    JmapFlowHalves {
        begin,
        complete: completion_with(outcome),
        bound: SMALL_BOUND,
    }
}

/// How long a test waits for the runtime's completion to reach the emit
/// seam.
const EMIT_BOUND: Duration = Duration::from_secs(5);

/// Waits for the next result on the channel, bounded.
fn next_result(rx: &std::sync::mpsc::Receiver<JmapSignInResult>) -> JmapSignInResult {
    rx.recv_timeout(EMIT_BOUND)
        .expect("the spawned half emitted its result")
}

#[test]
fn the_offer_probe_is_bounded_and_fails_soft() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");

    let never = jmap_offered_with(
        &runtime,
        SMALL_BOUND,
        "alice@example.test",
        None,
        |_email, _server_url| async move {
            let () = std::future::pending().await;
            unreachable!("the hanging probe never answers")
        },
    );
    assert!(!never, "a probe that never answers offers nothing");

    let refused = jmap_offered_with(
        &runtime,
        SMALL_BOUND,
        "alice@example.test",
        None,
        |_email, _server_url| async move { false },
    );
    assert!(!refused, "a server that advertises nothing offers nothing");

    let offered = jmap_offered_with(
        &runtime,
        SMALL_BOUND,
        "alice@example.test",
        None,
        |_email, _server_url| async move { true },
    );
    assert!(offered, "an advertising server offers the sign-in");
}

#[test]
fn a_begun_jmap_sign_in_opens_the_discovered_url_and_completes() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), fed_capture());
    let (_tx, results, emit) = emit_seam();

    jmap_begin_with(
        &desk,
        &runtime,
        emit,
        halves(
            discovered_begin(),
            JmapSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect("the browser opened the discovered url");

    assert_eq!(
        browser.urls(),
        vec![discovered_start().authorization_url],
        "the url that opened is the url the discovery minted"
    );
    let result = next_result(&results);
    assert_eq!(
        result.outcome,
        JmapSignInOutcome::Added {
            address: "alice@example.test".to_owned()
        },
        "the completion's outcome crosses the emit seam"
    );
}

#[test]
fn a_refused_completion_crosses_the_classification_word() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), fed_capture());
    let (_tx, results, emit) = emit_seam();

    jmap_begin_with(
        &desk,
        &runtime,
        emit,
        halves(
            discovered_begin(),
            JmapSignInOutcome::Refused {
                refusal: crate::oauth::commands::RefusalKind::Dead,
                detail: "the refresh token was refused".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect("the browser opened the discovered url");

    let result = next_result(&results);
    match result.outcome {
        JmapSignInOutcome::Refused { refusal, detail } => {
            assert_eq!(refusal, crate::oauth::commands::RefusalKind::Dead);
            assert!(
                detail.contains("refresh token"),
                "the detail rides: {detail}"
            );
        }
        other @ JmapSignInOutcome::Added { .. } => {
            panic!("the refusal crosses whole, got {other:?}")
        }
    }
}

#[test]
fn a_failed_discovery_refuses_and_opens_nothing() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());
    let failing: JmapBegin = Arc::new(|_email, _server_url| {
        Box::pin(async move {
            Err(AccountError::Jmap(
                "the server publishes no authorization server metadata".to_owned(),
            ))
        })
    });

    let error = jmap_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            failing,
            JmapSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect_err("the discovery's refusal is the begin's");

    assert!(
        matches!(error, OAuthCommandError::Discover(ref said) if said.contains("authorization server")),
        "the discovery's own sentence rides: {error}"
    );
    assert!(
        browser.urls().is_empty(),
        "a failed discovery opens no browser"
    );
}

#[test]
fn a_discovery_that_outlives_its_bound_refuses_and_leaves_nothing_in_flight() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());

    let error = jmap_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            hanging_begin(),
            JmapSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect_err("the bounded discovery refuses");

    assert!(
        matches!(error, OAuthCommandError::Discover(_)),
        "the bound's refusal is a discovery refusal: {error}"
    );
    // Nothing stayed in flight: a retry with an answering begin opens.
    jmap_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            discovered_begin(),
            JmapSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect("the retry begins fresh");
}

#[test]
fn one_flow_at_a_time_carries_to_the_jmap_begin() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());
    let (_tx, _results, emit) = emit_seam();

    jmap_begin_with(
        &desk,
        &runtime,
        emit,
        halves(
            discovered_begin(),
            JmapSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect("the first flow begins");

    let error = jmap_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            discovered_begin(),
            JmapSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        "alice@example.test",
        None,
    )
    .expect_err("one flow at a time");

    assert!(
        matches!(error, OAuthCommandError::InFlight),
        "the standing flow refuses the second begin: {error}"
    );
}

/// The emit seam as the tests hold it: one channel the result lands in.
fn emit_seam() -> (
    std::sync::mpsc::Sender<JmapSignInResult>,
    std::sync::mpsc::Receiver<JmapSignInResult>,
    impl FnOnce(JmapSignInResult) + Send + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let sender = tx.clone();
    let emit = move |result: JmapSignInResult| {
        let _ = sender.send(result);
    };
    (tx, rx, emit)
}
