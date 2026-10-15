// SPDX-License-Identifier: GPL-3.0-only
//! Google-shaped sources and endpoints for this module's tests: the
//! shared [`GraphTokenSource`] built over a Google provider config
//! (carrying the Desktop client's secret when the test wants one), plus
//! the one mock the Google deltas need that the graph module's shared
//! test support does not carry: an endpoint that records the whole text
//! of every request it serves, so a test can read exactly what a refresh
//! presented.

use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use astra_oauth::{OAuthClient, OAuthProviderConfig, GOOGLE_SCOPES};

use crate::{CredentialOrigin, GraphTokenSource, TokenSink};
use engine_api::AccountId;

/// A **distinct** account for every test source.
///
/// Not cosmetic. Token state is shared per account across a process, so
/// two sources built with the same id share one state, which is the
/// property under test in one place and a hidden coupling between
/// unrelated tests everywhere else: a failure memo left by one would
/// leak into the next, green or red depending on which order the
/// harness happened to run them in.
fn unique_account() -> AccountId {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    AccountId::try_from(format!("alice+{n}@example.com@google").as_str())
        .expect("a valid account id")
}

/// A Google token source whose refreshes go to `token_endpoint`, with
/// the Desktop client's non-confidential secret when `client_secret`
/// names one: the two halves of the shape whose difference is the
/// Google delta over the graph source's test sources.
pub(crate) fn google_source_at(
    token_endpoint: String,
    client_secret: Option<&str>,
    sink: Option<Arc<dyn TokenSink>>,
) -> Arc<GraphTokenSource> {
    let mut provider = OAuthProviderConfig::google(
        "google-client",
        client_secret.map(str::to_owned),
        "com.googleusercontent.apps.google-client:/oauth2redirect",
        GOOGLE_SCOPES,
    );
    provider.token_endpoint = token_endpoint;
    GraphTokenSource::from_parts(
        OAuthClient::new(provider).expect("the test provider config builds"),
        unique_account(),
        "initial-refresh".to_owned(),
        sink,
        "google",
        CredentialOrigin::Stored,
    )
}

/// A mock token endpoint that answers every request with `body` and
/// records the full text of each request it served: the shape that can
/// prove whether a refresh presented the client secret.
///
/// Returns the endpoint URL and the requests it served.
pub(crate) fn capturing_token_endpoint(body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let requests_thread = Arc::clone(&requests);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            requests_thread.lock().unwrap().push(request);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}/token"), requests)
}

/// Reads a whole small HTTP request, honouring `Content-Length`: a
/// single `read` can stop on the headers and leave the form body behind,
/// which would make the capture above record half a refresh.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(read) = stream.read(&mut buf) {
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..read]);
        let text = String::from_utf8_lossy(&raw);
        let Some((head, body)) = text.split_once("\r\n\r\n") else {
            continue;
        };
        let length: usize = head
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length: ")
                    .or_else(|| line.strip_prefix("content-length: "))
            })
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
        if body.len() >= length {
            break;
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}
