// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the Google arm's begin and capture: the authorization URL
//! the browser opens carries the loopback redirect with the port the
//! listener bound, the redirect the listener reads off the wire reaches
//! the completion whole, a deep-link delivery never feeds a Google flow,
//! and the Microsoft arm keeps the registered custom scheme.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use astra_oauth::{OAuthClient, OAuthProviderConfig};

use super::wire_tests::bare_completion;
use super::{
    begin_with, cancel_with, emit_seam, quiet_capture, refused_consent_capture, surface,
    ProviderKind, RefusalKind, SignInOutcome, SystemBrowser, EMIT_BOUND,
};

/// A browser stand-in that records every URL it opened: the begin tests
/// read the authorization URL back off the seam the product opens it
/// through, never out of the session's internals.
struct CapturingBrowser {
    urls: Mutex<Vec<String>>,
}

impl CapturingBrowser {
    /// A stand-in that opens and remembers.
    fn opening() -> Arc<Self> {
        Arc::new(Self {
            urls: Mutex::new(Vec::new()),
        })
    }

    /// The one URL this browser opened.
    fn sole_url(&self) -> String {
        let urls = self.urls.lock().unwrap();
        assert_eq!(urls.len(), 1, "the browser opened exactly one URL");
        urls[0].clone()
    }
}

impl SystemBrowser for CapturingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.urls.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

/// The browser stand-in as the surface holds it.
fn as_seam(browser: &Arc<CapturingBrowser>) -> Arc<dyn SystemBrowser> {
    Arc::clone(browser) as Arc<dyn SystemBrowser>
}

/// A Google Desktop client template: the registration shape the loopback
/// arm rebuilds per flow, never exchanged against in these tests (every
/// driven completion refuses at the core's parser).
fn google_client() -> Arc<OAuthClient> {
    let provider = OAuthProviderConfig::google(
        "123456789012-fixture.apps.googleusercontent.com",
        Some("the-non-confidential-desktop-secret".to_owned()),
        "http://127.0.0.1/",
        &["https://www.googleapis.com/auth/userinfo.email"],
    );
    Arc::new(OAuthClient::new(provider).expect("the test client builds"))
}

/// The redirect URI one opened authorization URL carries, read out of
/// its query the way the provider reads it.
fn redirect_of(authorization_url: &str) -> String {
    let parsed = url::Url::parse(authorization_url).expect("the authorization URL parses");
    parsed
        .query_pairs()
        .find(|(key, _)| key == "redirect_uri")
        .expect("the authorization URL carries its redirect")
        .1
        .into_owned()
}

/// The port the opened authorization URL's loopback redirect bound.
fn loopback_port_of(authorization_url: &str) -> u16 {
    let redirect =
        url::Url::parse(&redirect_of(authorization_url)).expect("the redirect parses as a URL");
    assert_eq!(redirect.scheme(), "http", "the Google redirect is loopback");
    assert_eq!(
        redirect.host_str(),
        Some("127.0.0.1"),
        "the Google redirect is loopback"
    );
    redirect
        .port()
        .expect("the loopback redirect carries its port")
}

/// Plays the browser's redirect: lands one callback request on the
/// loopback port the authorization URL named, and hands back the page
/// the listener answered with.
fn land_redirect(port: u16, target: &str) -> String {
    let mut browser = TcpStream::connect(("127.0.0.1", port)).expect("the loopback port accepts");
    browser
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .expect("the redirect request writes");
    let mut page = String::new();
    browser
        .read_to_string(&mut page)
        .expect("the close page reads back");
    page
}

/// How long a Google flow must sit quiet while a deep-link delivery
/// waits in the shared channel: the wrong capture would answer within
/// microseconds, so a small bound separates the arms without flaking a
/// healthy run (a healthy Google flow emits nothing until its own
/// listener captures).
const NO_EMIT_BOUND: Duration = Duration::from_millis(400);

#[test]
fn a_google_begin_opens_a_url_whose_redirect_is_the_bound_loopback_port() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CapturingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Google, google_client())],
        as_seam(&browser),
        quiet_capture(),
    );
    let (_tx, results, emit) = emit_seam();

    let opened = begin_with(
        &desk,
        &runtime,
        emit,
        bare_completion(),
        ProviderKind::Google,
        None,
    )
    .expect("the browser opened for the Google flow");
    assert_eq!(opened.provider, ProviderKind::Google);
    assert_eq!(
        browser.urls.lock().unwrap().len(),
        1,
        "the system browser opened exactly once"
    );

    let url = browser.sole_url();
    let port = loopback_port_of(&url);

    // The listener behind that port captures the redirect and answers
    // the close page, and the URL it read reaches the completion: the
    // emitted refusal carries the code out of the loopback URL's own
    // query, which only the completion's parser could have read.
    let page = land_redirect(
        port,
        "/?state=stub-state&error=access_denied&error_description=loopback",
    );
    assert!(
        page.starts_with("HTTP/1.1 200 OK"),
        "the listener answers the redirect with the close page: {page}"
    );
    assert!(
        page.contains("Astra Mail"),
        "the close page names the app the person returns to: {page}"
    );

    let result = results
        .recv_timeout(EMIT_BOUND)
        .expect("the listener's capture completes the flow");
    assert_eq!(result.provider, ProviderKind::Google);
    match result.outcome {
        SignInOutcome::Refused { refusal, detail } => {
            assert_eq!(refusal, RefusalKind::Indeterminate);
            assert!(
                detail.contains("access_denied"),
                "the captured URL's own query rides in the detail: {detail}"
            );
        }
        other => panic!("the withdrawn consent must refuse, got {other:?}"),
    }
}

#[test]
fn a_google_flow_does_not_consume_a_deep_link_event() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CapturingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Google, google_client())],
        as_seam(&browser),
        refused_consent_capture(),
    );
    let (_tx, results, emit) = emit_seam();

    begin_with(
        &desk,
        &runtime,
        emit,
        bare_completion(),
        ProviderKind::Google,
        None,
    )
    .expect("the browser opened for the Google flow");

    // A deep-link delivery waits in the shared channel; a Google flow
    // waits on its own listener, so the delivery must not complete it.
    assert!(
        results.recv_timeout(NO_EMIT_BOUND).is_err(),
        "a deep-link delivery never completes a Google flow"
    );

    // The flow still completes over its own capture.
    let port = loopback_port_of(&browser.sole_url());
    land_redirect(
        port,
        "/?state=stub-state&error=access_denied&error_description=loopback",
    );
    let result = results
        .recv_timeout(EMIT_BOUND)
        .expect("the flow's own listener captures completes it");
    assert_eq!(result.provider, ProviderKind::Google);
}

#[test]
fn a_loopback_capture_with_a_forged_state_is_refused() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CapturingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Google, google_client())],
        as_seam(&browser),
        quiet_capture(),
    );
    let (_tx, results, emit) = emit_seam();

    begin_with(
        &desk,
        &runtime,
        emit,
        bare_completion(),
        ProviderKind::Google,
        None,
    )
    .expect("the browser opened for the Google flow");

    let port = loopback_port_of(&browser.sole_url());
    land_redirect(port, "/?code=abc.def&state=someone-elses-state");
    let result = results
        .recv_timeout(EMIT_BOUND)
        .expect("the captured redirect reaches the completion");
    match result.outcome {
        SignInOutcome::Refused { refusal, detail } => {
            assert_eq!(refusal, RefusalKind::Indeterminate);
            assert!(
                detail.contains("state mismatch"),
                "the forged state is refused by the core's parser: {detail}"
            );
        }
        other => panic!("a forged state must refuse the flow, got {other:?}"),
    }
}

#[test]
fn a_microsoft_begin_keeps_the_registered_custom_scheme_redirect() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = CapturingBrowser::opening();
    let desk = surface(
        vec![(ProviderKind::Microsoft, super::client())],
        as_seam(&browser),
        quiet_capture(),
    );
    let (_tx, _results, emit) = emit_seam();

    begin_with(
        &desk,
        &runtime,
        emit,
        bare_completion(),
        ProviderKind::Microsoft,
        None,
    )
    .expect("the browser opened for the Microsoft flow");

    assert_eq!(
        redirect_of(&browser.sole_url()),
        super::OAUTH_REDIRECT_URI,
        "the Microsoft arm rides the registered scheme, untouched by the Google arm"
    );
    assert!(cancel_with(&desk), "the standing flow cancels");
}
