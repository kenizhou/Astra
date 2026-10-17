// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the jmap dial: the one provider it connects over a scripted
//! session, the verdict a refused credential carries out of it, and the
//! housekeeping failures that are nobody's outage.

use std::{
    io::{Read, Write},
    net::TcpListener,
};

use engine_api::Provider;
use provider_jmap::JmapError;

use super::dial_jmap_account;
use crate::{load_str, AccountConfig, AccountError};

/// A minimal session document advertising mail for one account, served by
/// a server at `port`: enough for the dial to connect and snapshot real
/// capabilities from.
fn session_body(port: u16) -> String {
    let mail = "urn:ietf:params:jmap:mail";
    format!(
        "{{\"apiUrl\":\"http://127.0.0.1:{port}/api\",\
         \"capabilities\":{{\"urn:ietf:params:jmap:core\":{{}},\"{mail}\":{{}}}},\
         \"primaryAccounts\":{{\"{mail}\":\"A1\"}},\
         \"accounts\":{{\"A1\":{{\"name\":\"scripted\",\"accountCapabilities\":{{\"{mail}\":{{}}}}}}}}}}",
    )
}

/// A scripted JMAP server answering every request with `status`, `headers`,
/// and the body `body_for` builds for its port: the offline stand-in for
/// the session endpoint a dial discovers. Returns its base URL.
fn scripted_server(
    status: &'static str,
    headers: &'static str,
    body_for: impl Fn(u16) -> String + Send + 'static,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    std::thread::spawn(move || {
        let body = body_for(addr.port());
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

/// A session endpoint that serves a mail session: what a healthy JMAP
/// account dials.
fn session_endpoint() -> String {
    scripted_server("200 OK", "", session_body)
}

/// A session endpoint that answers `401` offering the scheme the stored
/// credential already used: the server refusing the secret itself, not its
/// framing.
fn refusing_endpoint() -> String {
    scripted_server(
        "401 Unauthorized",
        "WWW-Authenticate: Basic realm=\"jmap\"\r\n",
        |_| "{}".to_owned(),
    )
}

/// A base URL nothing listens on, so a connection to it is refused before
/// any byte of the request is written.
fn refused_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    drop(listener);
    format!("http://{addr}")
}

/// A jmap account dialling `base_url`, the TLS policy every offline test
/// dials under.
fn account_at(base_url: &str) -> AccountConfig {
    load_str(&format!(
        "[jmap]\nemail = \"me@example.net\"\nbase_url = \"{base_url}\"\npassword = \"pw\"\n"
    ))
    .expect("a valid jmap document")
}

fn tls() -> engine_tls::TlsClientConfig {
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[])
        .expect("a policy with no exceptions")
}

#[tokio::test]
async fn the_dial_connects_one_provider_and_snapshots_the_sessions_capabilities() {
    let provider = dial_jmap_account(&account_at(&session_endpoint()), &tls())
        .await
        .expect("a connected provider");
    // The session advertised mail for its account, and the provider
    // answers with that snapshot, read from the session rather than
    // guessed: the one provider covers the whole account.
    let info = provider.connection_info();
    assert!(info.capabilities.mail());
    assert!(!info.capabilities.calendars());
}

#[tokio::test]
async fn a_refused_credential_is_a_rejected_signin() {
    // The challenge offers the scheme the credential already used, so the
    // engine has nothing to re-present: the server is refusing the secret
    // itself, which is the "sign in again" verdict.
    let err = dial_jmap_account(&account_at(&refusing_endpoint()), &tls())
        .await
        .expect_err("a refused credential");
    assert!(
        matches!(err, AccountError::SigninRejected(_)),
        "not rendered as a refusal: {err}"
    );
}

#[tokio::test]
async fn an_unreachable_session_is_not_a_rejected_signin() {
    // A connection that never opened says nothing about the credential:
    // it is an outage, and prompting for a new password over it would be
    // the false prompt.
    let err = dial_jmap_account(&account_at(&refused_endpoint()), &tls())
        .await
        .expect_err("an unreachable server");
    assert!(
        matches!(err, AccountError::Jmap(_)),
        "read as a refused credential: {err}"
    );
}

#[tokio::test]
async fn a_malformed_session_is_not_a_rejected_signin() {
    let endpoint = scripted_server("200 OK", "", |_| "{\"accounts\":{}}".to_owned());
    let err = dial_jmap_account(&account_at(&endpoint), &tls())
        .await
        .expect_err("a session without an apiUrl");
    assert!(
        matches!(err, AccountError::Jmap(_)),
        "read as a refused credential: {err}"
    );
}

#[tokio::test]
async fn the_dial_of_an_account_without_a_jmap_section_says_so() {
    let config =
        load_str("[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\nusername=\"u\"\npassword=\"p\"\n")
            .expect("a valid imap document");
    let err = dial_jmap_account(&config, &tls())
        .await
        .expect_err("no jmap section");
    assert!(
        matches!(err, AccountError::Jmap(ref said) if said.contains("jmap")),
        "not said in terms of the section: {err}"
    );
}

#[tokio::test]
async fn the_dial_of_a_section_without_a_secret_names_the_missing_credential() {
    let config =
        load_str("[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\n")
            .expect("a valid jmap document");
    let err = dial_jmap_account(&config, &tls())
        .await
        .expect_err("no stored secret");
    assert!(
        matches!(err, AccountError::MissingCredential("jmap")),
        "not named as the jmap credential: {err}"
    );
}

/// A JMAP HTTP status failure. Built variant-first because the engine's
/// own constructor is crate-private.
fn jmap_status(status: u16) -> JmapError {
    JmapError::Status {
        status,
        body: "{}".to_owned(),
        retry_after: None,
    }
}

#[test]
fn a_jmap_401_is_a_rejected_signin() {
    assert_eq!(
        jmap_status(401).failure_class(),
        engine_api::FailureClass::Authentication,
        "the engine no longer classifies a 401 as an authentication failure"
    );
    assert!(matches!(
        AccountError::from_jmap_connect(&jmap_status(401)),
        AccountError::SigninRejected(_)
    ));
}

#[test]
fn a_jmap_connect_that_failed_some_other_way_is_not_a_rejected_signin() {
    for err in [jmap_status(503), JmapError::Session("no apiUrl".to_owned())] {
        let class = err.failure_class();
        assert!(
            matches!(AccountError::from_jmap_connect(&err), AccountError::Jmap(_)),
            "a {class:?} failure was read as a refused credential"
        );
    }
}
