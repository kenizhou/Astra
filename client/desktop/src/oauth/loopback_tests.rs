// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the loopback listener: the flow's client is minted over a
//! bound loopback redirect, the one callback is captured whole and
//! answered with the close page, a later connection is refused (the
//! one-shot discipline), a probe that carries no callback leaves the
//! listener standing, the passing bound names its failure, and the
//! request parser keeps the callback shape. The tests talk to the
//! listener over real sockets on the loopback it bound, the browser's
//! own path.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use astra_oauth::{OAuthClient, OAuthProviderConfig};

use super::{begin, parse_request_target, LoopbackCapture, LoopbackFlow, TEMPLATE_REDIRECT_URI};

/// How long the tests stand their listeners: long enough for a real
/// request to land, never the production five minutes.
const TEST_BOUND: Duration = Duration::from_secs(2);

/// A Google Desktop client template: the registration shape [`begin`]
/// rebuilds over the bound port.
fn template() -> OAuthClient {
    let provider = OAuthProviderConfig::google(
        "123456789012-fixture.apps.googleusercontent.com",
        Some("the-non-confidential-desktop-secret".to_owned()),
        TEMPLATE_REDIRECT_URI,
        &["https://www.googleapis.com/auth/userinfo.email"],
    );
    OAuthClient::new(provider).expect("the template client builds")
}

/// The port one begun flow's redirect bound.
fn port_of(flow: &LoopbackFlow) -> u16 {
    url::Url::parse(&flow.client.provider().redirect_uri)
        .expect("the flow's redirect parses")
        .port()
        .expect("the loopback redirect carries its port")
}

/// Plays one request against the flow's port and reads the answer back.
fn request(port: u16, target: &str) -> String {
    let mut browser = TcpStream::connect(("127.0.0.1", port)).expect("the loopback port accepts");
    browser
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .expect("the request writes");
    let mut response = String::new();
    browser
        .read_to_string(&mut response)
        .expect("the answer reads back");
    response
}

#[test]
fn begin_mints_a_client_over_a_bound_loopback_redirect() {
    let flow = begin(&template(), TEST_BOUND).expect("the loopback flow begins");
    let redirect =
        url::Url::parse(&flow.client.provider().redirect_uri).expect("the redirect parses");
    assert_eq!(redirect.scheme(), "http", "the redirect is a bare URL");
    assert_eq!(
        redirect.host_str(),
        Some("127.0.0.1"),
        "the redirect names the IPv4 loopback"
    );
    assert!(
        redirect.port().is_some_and(|port| port > 0),
        "the redirect carries the port the listener bound"
    );
}

#[test]
fn the_listener_captures_one_callback_whole_and_answers_the_close_page() {
    let mut flow = begin(&template(), TEST_BOUND).expect("the loopback flow begins");
    let port = port_of(&flow);

    let page = request(port, "/?code=abc.def-ghi&state=xyz123");
    assert!(
        page.starts_with("HTTP/1.1 200 OK"),
        "the captured redirect is answered with the close page: {page}"
    );
    assert!(
        page.contains("Astra Mail"),
        "the close page names the app the person returns to: {page}"
    );

    let captured = flow
        .capture
        .blocking_recv()
        .expect("the capture crosses to the flow's waiter");
    assert_eq!(
        captured,
        LoopbackCapture::Received(format!(
            "http://127.0.0.1:{port}/?code=abc.def-ghi&state=xyz123"
        )),
        "the captured URL is the request target made absolute, whole"
    );
}

#[test]
fn a_second_connection_after_the_capture_is_refused() {
    let mut flow = begin(&template(), TEST_BOUND).expect("the loopback flow begins");
    let port = port_of(&flow);
    request(port, "/?code=abc&state=xyz");
    let captured = flow.capture.blocking_recv().expect("the callback captures");
    assert!(matches!(captured, LoopbackCapture::Received(_)));

    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_err(),
        "the one-shot listener refuses a connection after its capture"
    );
}

#[test]
fn a_probe_without_a_callback_shape_leaves_the_listener_standing() {
    let mut flow = begin(&template(), TEST_BOUND).expect("the loopback flow begins");
    let port = port_of(&flow);

    let probe = request(port, "/favicon.ico");
    assert!(
        probe.starts_with("HTTP/1.1 400 Bad Request"),
        "a request without the callback's shape is refused in place: {probe}"
    );

    let page = request(port, "/?error=access_denied&state=xyz");
    assert!(
        page.starts_with("HTTP/1.1 200 OK"),
        "the listener still stands for the real callback: {page}"
    );
    let captured = flow.capture.blocking_recv().expect("the callback captures");
    assert_eq!(
        captured,
        LoopbackCapture::Received(format!(
            "http://127.0.0.1:{port}/?error=access_denied&state=xyz"
        ))
    );
}

#[test]
fn the_passing_bound_names_the_failure() {
    let mut flow =
        begin(&template(), Duration::from_millis(100)).expect("the loopback flow begins");
    match flow.capture.blocking_recv() {
        Some(LoopbackCapture::Failed(sentence)) => assert!(
            sentence.contains("bound"),
            "the failure names its bound: {sentence}"
        ),
        other => panic!("a passing bound must fail the capture, got {other:?}"),
    }
}

#[test]
fn the_request_parser_keeps_the_callback_shape() {
    assert!(parse_request_target(b"GET /favicon.ico HTTP/1.1\r\n\r\n").is_none());
    assert!(parse_request_target(b"POST /?code=x&state=y HTTP/1.1\r\n\r\n").is_none());
    assert!(parse_request_target(b"GET /?state=y HTTP/1.1\r\n\r\n").is_none());
    assert_eq!(
        parse_request_target(b"GET /?error=access_denied&state=y HTTP/1.1\r\n\r\n"),
        Some("/?error=access_denied&state=y")
    );
}
