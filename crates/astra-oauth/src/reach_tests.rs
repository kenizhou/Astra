// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reach classification, driven through a real `reqwest` so
//! the property under test (how this transport reports each failure) is
//! exercised, not assumed.

use time::OffsetDateTime;

use crate::{AuthStyle, OAuthClient, OAuthError, OAuthProviderConfig, TokenRequestReach};

/// A client whose token endpoint is `endpoint`, for the reach tests below.
fn client_for(endpoint: &str) -> OAuthClient {
    OAuthClient::new(OAuthProviderConfig {
        authorize_endpoint: "https://example/authorize".to_owned(),
        token_endpoint: endpoint.to_owned(),
        client_id: "client-abc".to_owned(),
        client_secret: None,
        redirect_uri: "com.kenizhou.astra://oauth".to_owned(),
        scopes: vec!["offline_access".to_owned()],
        resource: None,
        expected_issuer: None,
        style: AuthStyle::Microsoft,
    })
    .unwrap()
}

/// The reach tests below drive a **real** `reqwest` rather than
/// hand-building errors, because the property under test is exactly how
/// this version of `reqwest` reports each failure. `reach()` reading
/// `is_connect()` is an assumption about a dependency, and an assumption a
/// test cannot falsify is a comment.
///
/// A name that cannot resolve is the case that must stay freely retryable:
/// the failure is a local resolver error, so no server saw the refresh
/// token, and classifying it as possibly-processed would park an account
/// over a failure that never left the device.
#[tokio::test]
async fn a_dns_failure_provably_never_left_the_device() {
    // `.invalid` is reserved by RFC 2606 and must never resolve.
    let client = client_for("https://astra-token-endpoint.invalid/token");
    let err = client
        .refresh("refresh-abc", OffsetDateTime::UNIX_EPOCH)
        .await
        .unwrap_err();
    assert_eq!(
        err.reach(),
        TokenRequestReach::NotSent,
        "a name that cannot resolve reached no server: {err}",
    );
}

#[tokio::test]
async fn a_refused_connection_provably_never_left_the_device() {
    // Bind to claim a port, then drop the listener so nothing is
    // accepting on it.
    let addr = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let client = client_for(&format!("http://{addr}/token"));
    let err = client
        .refresh("refresh-abc", OffsetDateTime::UNIX_EPOCH)
        .await
        .unwrap_err();
    assert_eq!(
        err.reach(),
        TokenRequestReach::NotSent,
        "a refused connection reached no server: {err}",
    );
}

/// The dangerous one, and the reason `ResponseLost` exists: the server
/// answered, so its handler ran and rotated the refresh token, and then
/// the body died on the way back. Nothing in the error text distinguishes
/// this from a failure to connect, and the consequences are opposite.
#[tokio::test]
async fn a_response_that_dies_mid_body_may_already_have_been_processed() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            // Promise 500 bytes, send 4, hang up. The status line arrives,
            // the body never does.
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 500\r\n\r\n{\"a\"",
            );
        }
    });
    let client = client_for(&format!("http://{addr}/token"));
    let err = client
        .refresh("refresh-abc", OffsetDateTime::UNIX_EPOCH)
        .await
        .unwrap_err();
    assert!(
        matches!(err, OAuthError::ResponseLost(_)),
        "a truncated body must not be reported as a transport failure: {err:?}",
    );
    assert_eq!(err.reach(), TokenRequestReach::MaybeProcessed);
}

#[test]
fn a_server_side_error_may_already_have_been_processed() {
    // A 5xx arrives as `Endpoint` (there is no OAuth error body to parse),
    // and a server that failed *after* rotating looks exactly like one
    // that failed before.
    let err = OAuthError::Endpoint {
        error: "http 500".to_owned(),
        description: None,
    };
    assert_eq!(err.reach(), TokenRequestReach::MaybeProcessed);
    // A 2xx body we could not parse held the rotation we needed.
    assert_eq!(
        OAuthError::Decode("expected value".to_owned()).reach(),
        TokenRequestReach::MaybeProcessed,
    );
}
