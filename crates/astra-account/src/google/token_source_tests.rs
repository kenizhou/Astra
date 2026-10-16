// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the Google token source's deltas over the shared source:
//! the Desktop secret's ride on the refresh, and the rotation backstop
//! over a server that does rotate. The single-flight, the skew, and the
//! refusal classification are the shared source's own, proven by the
//! graph module's tests; these are the halves that differ for Google.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engine_api::AccountId;

use super::google_token_source;
use crate::google::test_support::{capturing_token_endpoint, google_source_at};
use crate::graph::token_source::test_support::ratcheting_token_endpoint;
use crate::{load_str, CredentialOrigin, TokenSink};

/// A healthy refresh answer that names neither a rotated refresh token
/// nor a scope: the shape an ordinary Google refresh returns.
const OK_NO_ROTATION: &str = r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT"}"#;

/// What a sink was told: each rotated refresh token.
#[derive(Default)]
struct Recorder {
    rotated: Mutex<Vec<String>>,
}

#[async_trait]
impl TokenSink for Recorder {
    async fn refresh_token_rotated(&self, _account: &AccountId, new_refresh_token: &str) {
        self.rotated
            .lock()
            .unwrap()
            .push(new_refresh_token.to_owned());
    }

    async fn scopes_granted(&self, _account: &AccountId, _granted: &astra_oauth::GrantedScopes) {}
}

#[tokio::test]
async fn the_refresh_presents_the_stored_desktop_secret() {
    // The Google delta over the graph source: the token endpoint rejects
    // the Desktop client's refresh without the non-confidential secret,
    // so it rides the refresh grant exactly as it rode the exchange.
    let (endpoint, requests) = capturing_token_endpoint(OK_NO_ROTATION);
    let source = google_source_at(endpoint, Some("GOCSPX-desktop-secret"), None);

    assert_eq!(source.access_token().await.unwrap(), "AT");
    let requests = requests.lock().unwrap();
    let body = &requests[0];
    assert!(body.contains("grant_type=refresh_token"), "{body}");
    assert!(
        body.contains("client_secret=GOCSPX-desktop-secret"),
        "the refresh left without the Desktop secret: {body}",
    );
}

#[tokio::test]
async fn a_secretless_source_sends_no_client_secret() {
    // A mobile-client registration needs no secret; its refreshes must
    // not carry a key the registration never had.
    let (endpoint, requests) = capturing_token_endpoint(OK_NO_ROTATION);
    let source = google_source_at(endpoint, None, None);

    source.access_token().await.unwrap();
    let body = &requests.lock().unwrap()[0];
    assert!(!body.contains("client_secret"), "{body}");
}

#[tokio::test]
async fn an_ordinary_google_refresh_reports_no_rotation() {
    // Google does not rotate its refresh tokens on a refresh grant, so
    // the ordinary refresh names none and the sink hears nothing.
    let recorder = Arc::new(Recorder::default());
    let (endpoint, _requests) = capturing_token_endpoint(OK_NO_ROTATION);
    let source = google_source_at(
        endpoint,
        Some("GOCSPX-desktop-secret"),
        Some(Arc::clone(&recorder) as Arc<dyn TokenSink>),
    );

    source.access_token().await.unwrap();
    assert!(
        recorder.rotated.lock().unwrap().is_empty(),
        "a refresh that named no refresh token was reported as a rotation",
    );
}

#[tokio::test]
async fn a_rotation_the_server_does_send_still_reaches_the_sink() {
    // The backstop: Google does not rotate in practice, but the write is
    // not optional, because a server that does rotate and is not answered
    // with a stored replacement kills the account at the next launch.
    // The ratcheting endpoint is exactly such a server.
    let (endpoint, _hits) = ratcheting_token_endpoint("initial-refresh");
    let recorder = Arc::new(Recorder::default());
    let source = google_source_at(
        endpoint,
        None,
        Some(Arc::clone(&recorder) as Arc<dyn TokenSink>),
    );

    assert_eq!(source.access_token().await.unwrap(), "AT-1");
    assert_eq!(recorder.rotated.lock().unwrap().as_slice(), ["rotated-1"]);
}

/// A stored Google account: the google section alone, carrying the
/// Desktop secret a launch-later refresh must still have.
const GOOGLE_DOC: &str = r#"
[google]
email = "Alice@Gmail.com"
client_id = "google-client"
client_secret = "GOCSPX-desktop"
redirect_uri = "http://127.0.0.1:49152/oauth2redirect"
scopes = ["https://mail.google.com/"]
refresh_token = "stored-refresh"
"#;

#[tokio::test]
async fn a_stored_document_builds_the_google_token_source() {
    // The document is the source's whole input: the section the sign-in
    // wrote is the section a boot builds the refreshes from, secret and
    // all. That the secret reaches the wire is the test above, over the
    // provider config this constructor reads it through.
    let config = load_str(GOOGLE_DOC).expect("a stored google account");
    let account = AccountId::try_from("alice@gmail.com@google").unwrap();
    let source = google_token_source(
        config.google.as_ref().expect("the google section"),
        account,
        None,
        CredentialOrigin::Stored,
    );
    assert!(source.is_ok(), "{}", source.unwrap_err());
}
