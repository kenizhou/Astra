// SPDX-License-Identifier: GPL-3.0-only
//! The lifecycle proof's scripted servers, split from the suite for the
//! 500-line cap: a stand-in `/me` and a rotating, ratcheting token
//! endpoint, both plain loopback HTTP so the production client stack (the
//! real reqwest over the shared TLS policy, the real refresh body) makes
//! the requests the proof pins.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use astra_oauth::{OAuthProviderConfig, Secret, TokenSet};

/// A scripted `/me` server: answers one canned profile body and records
/// each request's head, so the proof can pin the bearer the completion
/// handed the lookup.
pub(super) fn me_server(body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_thread = Arc::clone(&seen);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
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
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}"), seen)
}

/// Reads a whole small HTTP request, honouring `Content-Length`: a single
/// `read` can stop on the headers and leave the form body behind, which
/// would make the ratcheting server below decide a legitimate refresh was
/// a replay and turn the proof into a flake.
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

/// One `application/x-www-form-urlencoded` value out of a request body.
fn form_value(request: &str, key: &str) -> Option<String> {
    let (_, body) = request.split_once("\r\n\r\n")?;
    body.split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .map(str::to_owned)
}

/// A scripted token endpoint that models a **rotating, ratcheting**
/// authorization server: it holds exactly one currently-valid refresh
/// token, a refresh presenting it rotates and succeeds, and a refresh
/// presenting anything else is a replay that revokes the grant for good.
/// The only server shape on which the single-flight and the rotation
/// landing are observable rather than merely decorative offline.
pub(super) fn ratcheting_token_endpoint(initial: &str) -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_thread = Arc::clone(&hits);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut valid = initial.to_owned();
    let mut generation = 0usize;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            hits_thread.fetch_add(1, Ordering::SeqCst);
            let presented = form_value(&request, "refresh_token").unwrap_or_default();
            let (status, body) = if presented == valid && !valid.is_empty() {
                generation += 1;
                valid = format!("rotated-{generation}");
                (
                    "200 OK",
                    format!(
                        r#"{{"token_type":"Bearer","expires_in":3600,"access_token":"AT-{generation}","refresh_token":"{valid}"}}"#
                    ),
                )
            } else {
                valid.clear();
                (
                    "400 Bad Request",
                    r#"{"error":"invalid_grant","error_description":"ratchet or client_id mismatch"}"#
                        .to_owned(),
                )
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}/token"), hits)
}

/// The flow's provider config: the registration shape the wizard builds,
/// with its token endpoint pointed at the scripted server, which is the
/// one substitution an offline proof of the real refresh needs.
pub(super) fn provider_config_at(token_endpoint: &str) -> OAuthProviderConfig {
    let mut provider = OAuthProviderConfig::microsoft(
        "client-abc",
        "common",
        "com.kenizhou.astra://oauth",
        &[
            "offline_access",
            "https://graph.microsoft.com/Mail.ReadWrite",
            "https://graph.microsoft.com/Calendars.ReadWrite",
        ],
    );
    provider.token_endpoint = token_endpoint.to_owned();
    provider
}

/// The token set a completed exchange answers for `refresh` / `access`.
pub(super) fn token_set_with(refresh: &str, access: &str) -> TokenSet {
    TokenSet {
        access_token: Secret::new(access.to_owned()),
        refresh_token: Some(Secret::new(refresh.to_owned())),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        scope: "offline_access https://graph.microsoft.com/Mail.ReadWrite \
                https://graph.microsoft.com/Calendars.ReadWrite"
            .to_owned(),
        token_type: "Bearer".to_owned(),
    }
}
