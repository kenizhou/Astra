// SPDX-License-Identifier: GPL-3.0-only
//! The begin half's tests: the minted URL runs through the one session's
//! external arm, a refused or bound-out route opens nothing and leaves
//! nothing in flight, one flow stands at a time, and the completion's
//! outcome crosses the emit seam. The begin seam answers a minted start
//! and the completion is the caller's own, so no test touches a network.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use super::{setup_json, SMALL_BOUND};
use crate::oauth::commands::{OAuthCommandError, SignInSurface};
use crate::oauth::mail_add::MailCompletion;
use crate::oauth::mail_begin::{MailBegin, MailFlowHalves, MailLoginStart};
use crate::oauth::mail_commands::{mail_begin_with, MailSignInOutcome, MailSignInResult};

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
fn fed_capture(raw: &str) -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(raw.to_owned()).expect("the fed delivery lands");
    rx
}

/// The surface over a browser stand-in and a capture, carrying no
/// registered client (the mail flow needs none).
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

/// A callback delivery on the OAuth scheme, raw as the OS hands it over.
const RAW_CALLBACK: &str = "com.kenizhou.astra://oauth?code=abc.def-ghi&state=xyz123";

/// The start a scripted route answers, exactly what the production begin
/// mints.
fn discovered_start() -> MailLoginStart {
    MailLoginStart {
        authorization_url: "https://id.example.test/authorize?client_id=client-xyz".to_owned(),
        pending: r#"{"email":"alice@example.test"}"#.to_owned(),
    }
}

/// The begin seam answering the discovered start.
fn discovered_begin() -> MailBegin {
    Arc::new(|_config, _issuer| Box::pin(async move { Ok(discovered_start()) }))
}

/// The begin seam that never answers.
fn hanging_begin() -> MailBegin {
    Arc::new(|_config, _issuer| {
        Box::pin(async move {
            let () = std::future::pending().await;
            unreachable!("the hanging begin never answers")
        })
    })
}

/// The completion seam answering one outcome for any redirect.
fn completion_with(outcome: MailSignInOutcome) -> MailCompletion {
    Box::new(move |_pending, _callback| Box::pin(async move { outcome }))
}

/// The halves over one begin seam and one scripted outcome.
fn halves(begin: MailBegin, outcome: MailSignInOutcome) -> MailFlowHalves {
    MailFlowHalves {
        begin,
        complete: completion_with(outcome),
        bound: SMALL_BOUND,
    }
}

/// How long a test waits for the runtime's completion to reach the emit
/// seam.
const EMIT_BOUND: Duration = Duration::from_secs(5);

/// Waits for the next result on the channel, bounded.
fn next_result(rx: &std::sync::mpsc::Receiver<MailSignInResult>) -> MailSignInResult {
    rx.recv_timeout(EMIT_BOUND)
        .expect("the spawned half emitted its result")
}

/// The emit seam as the tests hold it: one channel the result lands in.
fn emit_seam() -> (
    std::sync::mpsc::Sender<MailSignInResult>,
    std::sync::mpsc::Receiver<MailSignInResult>,
    impl FnOnce(MailSignInResult) + Send + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let sender = tx.clone();
    let emit = move |result: MailSignInResult| {
        let _ = sender.send(result);
    };
    (tx, rx, emit)
}

#[test]
fn a_begun_mail_sign_in_opens_the_minted_url_and_completes() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), fed_capture(RAW_CALLBACK));
    let (_tx, results, emit) = emit_seam();

    mail_begin_with(
        &desk,
        &runtime,
        emit,
        halves(
            discovered_begin(),
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        setup_json(),
        None,
    )
    .expect("the browser opened the minted url");

    assert_eq!(
        browser.urls(),
        vec![discovered_start().authorization_url],
        "the url that opened is the url the route minted"
    );
    let result = next_result(&results);
    assert_eq!(
        result.outcome,
        MailSignInOutcome::Added {
            address: "alice@example.test".to_owned()
        },
        "the completion's outcome crosses the emit seam"
    );
}

#[test]
fn a_refused_route_refuses_and_opens_nothing() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());
    let failing: MailBegin = Arc::new(|_config, _issuer| {
        Box::pin(async move {
            Err(astra_account::AccountError::MailOauth(
                "no authorization server answered for example.test".to_owned(),
            ))
        })
    });

    let error = mail_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            failing,
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        setup_json(),
        None,
    )
    .expect_err("the route's refusal is the begin's");

    assert!(
        matches!(error, OAuthCommandError::Discover(ref said) if said.contains("authorization server")),
        "the route's own sentence rides: {error}"
    );
    assert!(
        browser.urls().is_empty(),
        "a refused route opens no browser"
    );
}

#[test]
fn a_route_that_outlives_its_bound_refuses_and_leaves_nothing_in_flight() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());

    let error = mail_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            hanging_begin(),
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        setup_json(),
        None,
    )
    .expect_err("the bounded route walk refuses");

    assert!(
        matches!(error, OAuthCommandError::Discover(_)),
        "the bound's refusal is a discovery refusal: {error}"
    );
    // Nothing stayed in flight: a retry with an answering begin opens.
    mail_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            discovered_begin(),
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        setup_json(),
        None,
    )
    .expect("the retry begins fresh");
}

#[test]
fn one_flow_at_a_time_carries_to_the_mail_begin() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());
    let (_tx, _results, emit) = emit_seam();

    mail_begin_with(
        &desk,
        &runtime,
        emit,
        halves(
            discovered_begin(),
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        setup_json(),
        None,
    )
    .expect("the first flow begins");

    let error = mail_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            discovered_begin(),
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        setup_json(),
        None,
    )
    .expect_err("one flow at a time");

    assert!(
        matches!(error, OAuthCommandError::InFlight),
        "the standing flow refuses the second begin: {error}"
    );
}

#[test]
fn a_setup_that_cannot_build_refuses_the_begin() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), quiet_capture());

    let error = mail_begin_with(
        &desk,
        &runtime,
        |_result| {},
        halves(
            discovered_begin(),
            MailSignInOutcome::Added {
                address: "alice@example.test".to_owned(),
            },
        ),
        serde_json::json!({"nothing": "here"}),
        None,
    )
    .expect_err("a setup that cannot build refuses the begin");

    assert!(
        matches!(error, OAuthCommandError::Discover(_)),
        "the unbuildable setup is a discovery refusal: {error}"
    );
    assert_eq!(browser.urls(), [] as [std::string::String; 0]);
}
