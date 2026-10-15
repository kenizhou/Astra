// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the Google sign-in address lookup: a grant holding
//! `userinfo.email` reads `oauth2/v3/userinfo` (its `email` field), a
//! grant without it falls back to the Gmail profile (its `emailAddress`
//! field), and a reply that names no address or answers a non-2xx is
//! refused. Every test drives the `_at` seam against a stand-in HTTP
//! server, so the request the product sends (the bearer token, the path)
//! is pinned exactly.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use super::fetch_google_address_at;

/// One stand-in profile server: records each request's head (the request
/// line and the headers) and answers the canned `status` + `body`
/// whichever path the read asked for, so one server serves both roots.
fn profile_server(status: &str, body: &str) -> (String, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_thread = Arc::clone(&seen);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let status = status.to_owned();
    let body = body.to_owned();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            // A GET has no body, so the head is the whole request.
            while let Ok(read) = stream.read(&mut buf) {
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..read]);
                if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&raw).into_owned();
            seen_thread.lock().unwrap().push(head);
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}"), seen)
}

/// The granted set carrying `userinfo.email`, the shape every sign-in
/// this build asks for produces.
fn granted_with_email() -> Vec<String> {
    vec![
        "https://mail.google.com/".to_owned(),
        "https://www.googleapis.com/auth/userinfo.email".to_owned(),
    ]
}

/// The granted set of a calendar-only account, or one issued before the
/// email scope was asked for: no userinfo read is allowed.
fn granted_without_email() -> Vec<String> {
    vec![
        "https://mail.google.com/".to_owned(),
        "https://www.googleapis.com/auth/calendar".to_owned(),
    ]
}

/// The request heads the server saw.
fn requests(seen: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    seen.lock().unwrap().clone()
}

#[tokio::test]
async fn a_grant_holding_the_email_scope_reads_userinfo() {
    let (base, _seen) = profile_server("200 OK", r#"{"email":"Alice@Gmail.example"}"#);
    let address = fetch_google_address_at(&base, &base, "the-access-token", &granted_with_email())
        .await
        .expect("the lookup answers");
    assert_eq!(address, "Alice@Gmail.example");
}

#[tokio::test]
async fn a_grant_without_the_email_scope_reads_the_gmail_profile() {
    let (base, _seen) = profile_server("200 OK", r#"{"emailAddress":"bob@gmail.example"}"#);
    let address =
        fetch_google_address_at(&base, &base, "the-access-token", &granted_without_email())
            .await
            .expect("the fallback names the account too");
    assert_eq!(address, "bob@gmail.example");
}

#[tokio::test]
async fn an_empty_address_reads_as_absent() {
    let (base, _seen) = profile_server("200 OK", r#"{"email":""}"#);
    let err = fetch_google_address_at(&base, &base, "the-access-token", &granted_with_email())
        .await
        .expect_err("an empty address names nothing");
    assert!(
        err.to_string().contains("no address"),
        "the refusal says why: {err}"
    );
}

#[tokio::test]
async fn a_reply_that_names_no_address_is_refused() {
    let (base, _seen) = profile_server("200 OK", r#"{"sub":"123456"}"#);
    let err = fetch_google_address_at(&base, &base, "the-access-token", &granted_with_email())
        .await
        .expect_err("a nameless account cannot be named");
    assert!(
        err.to_string().contains("no address"),
        "the refusal says why: {err}"
    );
}

#[tokio::test]
async fn a_non_success_status_is_refused() {
    let (base, _seen) = profile_server(
        "403 Forbidden",
        r#"{"error":{"code":403,"message":"access denied"}}"#,
    );
    let err = fetch_google_address_at(&base, &base, "the-access-token", &granted_with_email())
        .await
        .expect_err("a refused token refuses the lookup");
    assert!(
        err.to_string().contains("403"),
        "the status rides in the refusal: {err}"
    );
}

#[tokio::test]
async fn the_granted_read_carries_the_bearer_to_userinfo() {
    let (base, seen) = profile_server("200 OK", r#"{"email":"Alice@Gmail.example"}"#);
    fetch_google_address_at(&base, &base, "the-access-token", &granted_with_email())
        .await
        .expect("the lookup answers");
    let head = requests(&seen).first().cloned().unwrap_or_default();
    assert_eq!(requests(&seen).len(), 1, "exactly one read");
    assert!(
        head.starts_with("GET /oauth2/v3/userinfo "),
        "the granted set reads userinfo: {head}"
    );
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer the-access-token")),
        "the access token authenticates the read: {head}"
    );
}

#[tokio::test]
async fn the_fallback_read_carries_the_bearer_to_the_gmail_profile() {
    let (base, seen) = profile_server("200 OK", r#"{"emailAddress":"bob@gmail.example"}"#);
    fetch_google_address_at(&base, &base, "the-access-token", &granted_without_email())
        .await
        .expect("the fallback answers");
    let head = requests(&seen).first().cloned().unwrap_or_default();
    assert_eq!(requests(&seen).len(), 1, "exactly one read");
    assert!(
        head.starts_with("GET /gmail/v1/users/me/profile "),
        "the grant without the email scope reads the Gmail profile: {head}"
    );
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer the-access-token")),
        "the access token authenticates the fallback too: {head}"
    );
}
