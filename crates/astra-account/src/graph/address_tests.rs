// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sign-in address lookup: the `/me` read prefers the SMTP
//! address, falls back to the sign-in name when there is none, and refuses
//! a reply that names no address or answers a non-2xx. Every test drives
//! the `_at` seam against a stand-in HTTP server, so the request the
//! product sends (the bearer token, the `$select`) is pinned exactly.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use super::fetch_primary_address_at;

/// One stand-in `/me` server: records each request's head (the request
/// line and the headers) and answers the canned `status` + `body`.
fn me_server(status: &str, body: &str) -> (String, Arc<Mutex<Vec<String>>>) {
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

/// The request heads the server saw.
fn requests(seen: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    seen.lock().unwrap().clone()
}

#[tokio::test]
async fn the_smtp_address_wins_over_the_sign_in_name() {
    let (base, seen) = me_server(
        "200 OK",
        r#"{"mail":"alice@example.com","userPrincipalName":"alice@tenant.example.test"}"#,
    );
    let address = fetch_primary_address_at(&base, "the-access-token")
        .await
        .expect("the lookup answers");
    assert_eq!(address, "alice@example.com");
    assert_eq!(requests(&seen).len(), 1, "exactly one read");
}

#[tokio::test]
async fn a_mailbox_with_no_smtp_address_falls_back_to_the_sign_in_name() {
    let (base, _seen) = me_server(
        "200 OK",
        r#"{"mail":null,"userPrincipalName":"alice@tenant.example.test"}"#,
    );
    let address = fetch_primary_address_at(&base, "the-access-token")
        .await
        .expect("the lookup answers");
    assert_eq!(address, "alice@tenant.example.test");
}

#[tokio::test]
async fn an_empty_smtp_address_reads_as_absent() {
    let (base, _seen) = me_server(
        "200 OK",
        r#"{"mail":"","userPrincipalName":"alice@tenant.example.test"}"#,
    );
    let address = fetch_primary_address_at(&base, "the-access-token")
        .await
        .expect("the lookup answers");
    assert_eq!(address, "alice@tenant.example.test");
}

#[tokio::test]
async fn a_reply_that_names_no_address_is_refused() {
    let (base, _seen) = me_server("200 OK", r#"{"displayName":"Alice"}"#);
    let err = fetch_primary_address_at(&base, "the-access-token")
        .await
        .expect_err("a nameless account cannot be named");
    assert!(
        err.to_string().contains("no address"),
        "the refusal says why: {err}"
    );
}

#[tokio::test]
async fn a_non_success_status_is_refused() {
    let (base, _seen) = me_server(
        "403 Forbidden",
        r#"{"error":{"code":"InvalidAuthenticationToken"}}"#,
    );
    let err = fetch_primary_address_at(&base, "the-access-token")
        .await
        .expect_err("a refused token refuses the lookup");
    assert!(
        err.to_string().contains("403"),
        "the status rides in the refusal: {err}"
    );
}

#[tokio::test]
async fn the_read_carries_the_bearer_token_and_the_select() {
    let (base, seen) = me_server(
        "200 OK",
        r#"{"mail":"alice@example.com","userPrincipalName":"alice@tenant.example.test"}"#,
    );
    fetch_primary_address_at(&base, "the-access-token")
        .await
        .expect("the lookup answers");
    let head = requests(&seen).first().cloned().unwrap_or_default();
    assert!(
        head.starts_with("GET /me?$select=mail,userPrincipalName "),
        "the read selects exactly the two fields: {head}"
    );
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer the-access-token")),
        "the access token authenticates the read: {head}"
    );
}
