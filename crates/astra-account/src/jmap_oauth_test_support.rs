// SPDX-License-Identifier: GPL-3.0-only
//! The shared fixtures of the JMAP OAuth leg's tests: a scripted server
//! that plays the discovery chain's counterpart (the session's `401`
//! naming its resource metadata, the RFC 9728 and RFC 8414 documents, the
//! RFC 7591 registration, and the token exchange), and the stored sections
//! and callbacks the flows' tests hand around. Loopback `http`
//! throughout, the one host class the chain's HTTPS rule exempts.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

use time::OffsetDateTime;

use super::PendingJmapLogin;
use crate::{load_str, JmapAccount};

/// The redirect URI the platform auth session captures; the tests' stand-in
/// for the host's registered custom scheme.
pub(crate) const REDIRECT_URI: &str = "com.kenizhou.astra://jmap-oauth";

/// Which answer a scripted authorization server gives one request: the
/// status, extra headers, and body, or `None` to hang up. Receives the
/// server's own base URL, which the metadata documents name the server by.
pub(crate) type Answer = Option<(&'static str, String, String)>;

/// A scripted server that plays the discovery chain's counterpart and
/// records the path of every request it served, so a test can tell a probe
/// from a registration and one registration from two.
pub(crate) fn discovery_server(
    answers: fn(&str, &str, &str) -> Answer,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let base = format!("http://{}", listener.local_addr().expect("a bound address"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_thread = Arc::clone(&seen);
    let thread_base = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .split('?')
                .next()
                .unwrap_or("/")
                .to_owned();
            let method = request
                .split_whitespace()
                .next()
                .unwrap_or("GET")
                .to_owned();
            seen_thread.lock().expect("seen lock").push(path.clone());
            let Some((status, headers, body)) = answers(&thread_base, &path, &method) else {
                continue;
            };
            let response = format!(
                "HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (base, seen)
}

/// Reads a whole small HTTP request, honouring `Content-Length`, so a POST
/// body split from the headers never reads as an empty registration.
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

/// The full healthy chain: a `401` naming the resource metadata, both
/// metadata documents, open registration, and a token exchange that issues
/// a refresh token.
pub(crate) fn full_chain(base: &str, path: &str, method: &str) -> Answer {
    match (method, path) {
        ("GET", "/.well-known/jmap") => Some(session_401(base)),
        ("GET", "/.well-known/oauth-protected-resource") => Some((
            "200 OK",
            String::new(),
            format!(
                "{{\"resource\":\"{base}/jmap/session\",\"authorization_servers\":[\"{base}\"]}}"
            ),
        )),
        ("GET", "/.well-known/oauth-authorization-server") => {
            Some(("200 OK", String::new(), metadata(base, true)))
        }
        ("POST", "/register") => Some((
            "201 Created",
            String::new(),
            r#"{"client_id":"client-xyz"}"#.to_owned(),
        )),
        ("POST", "/token") => Some((
            "200 OK",
            String::new(),
            r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1","refresh_token":"rt-1","scope":"offline_access"}"#
                .to_owned(),
        )),
        _ => None,
    }
}

/// The session's `401`, naming the RFC 9728 document the chain reads next.
fn session_401(base: &str) -> (&'static str, String, String) {
    (
        "401 Unauthorized",
        format!(
            "WWW-Authenticate: Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\"\r\n"
        ),
        "{}".to_owned(),
    )
}

/// One authorization-server metadata document, with the registration
/// endpoint only when `registration` says the server offers it.
pub(crate) fn metadata(base: &str, registration: bool) -> String {
    let mail = "urn:ietf:params:oauth:scope:mail";
    let calendar = "urn:ietf:params:oauth:scope:calendar";
    format!(
        "{{\"issuer\":\"{base}\",\
         \"authorization_endpoint\":\"{base}/oauth/authorize\",\
         \"token_endpoint\":\"{base}/token\",\
         \"registration_endpoint\":{registration},\
         \"scopes_supported\":[\"offline_access\",\"{mail}\",\"{calendar}\",\"urn:example:admin\"],\
         \"code_challenge_methods_supported\":[\"S256\",\"plain\"],\
         \"authorization_response_iss_parameter_supported\":true}}",
        registration = if registration {
            format!("\"{base}/register\"")
        } else {
            "null".to_owned()
        },
    )
}

/// The chain with the token exchange's answer carrying no refresh token:
/// the exchange succeeds and leaves nothing to refresh with.
pub(crate) fn no_refresh_chain(base: &str, path: &str, method: &str) -> Answer {
    match (method, path) {
        (_, "/token") => Some((
            "200 OK",
            String::new(),
            r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1","scope":"offline_access"}"#
                .to_owned(),
        )),
        _ => full_chain(base, path, method),
    }
}

/// A chain whose advertised scopes include no mail capability: it
/// completes, but the grant would be good for nothing.
pub(crate) fn no_mail_chain(base: &str, path: &str, method: &str) -> Answer {
    match (method, path) {
        ("GET", "/.well-known/jmap") => Some(session_401(base)),
        ("GET", "/.well-known/oauth-protected-resource") => Some((
            "200 OK",
            String::new(),
            format!("{{\"authorization_servers\":[\"{base}\"]}}"),
        )),
        ("GET", "/.well-known/oauth-authorization-server") => Some((
            "200 OK",
            String::new(),
            format!(
                "{{\"issuer\":\"{base}\",\
                  \"authorization_endpoint\":\"{base}/oauth/authorize\",\
                  \"token_endpoint\":\"{base}/token\",\
                  \"registration_endpoint\":\"{base}/register\",\
                  \"scopes_supported\":[\"offline_access\",\"urn:example:contacts\"],\
                  \"code_challenge_methods_supported\":[\"S256\"]}}"
            ),
        )),
        ("POST", "/register") => Some((
            "201 Created",
            String::new(),
            r#"{"client_id":"client-xyz"}"#.to_owned(),
        )),
        _ => None,
    }
}

/// A base URL nothing listens on, so any request fails before it is sent.
pub(crate) fn refused_base() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    drop(listener);
    format!("http://{addr}")
}

pub(crate) fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// The callback the authorization server redirects back to, echoing the
/// `state` a pending handle carries and the `iss` it demands (RFC 9207,
/// only when the grant recorded one).
pub(crate) fn callback_for(pending: &str, code: &str) -> String {
    let pending: PendingJmapLogin = serde_json::from_str(pending).expect("a decodable handle");
    let iss = pending
        .issuer
        .as_deref()
        .map(|issuer| format!("&iss={}", issuer.replace(':', "%3A").replace('/', "%2F")))
        .unwrap_or_default();
    format!("{REDIRECT_URI}?code={code}&state={}{}", pending.state, iss)
}

/// A stored OAuth section at `base_url`, the shape a completed sign-in
/// leaves in the secure store: a grant with every field a re-auth
/// replays, including the RFC 8707 resource.
pub(crate) fn stored_oauth_account(email: &str, base_url: &str) -> JmapAccount {
    let doc = format!(
        "[jmap]\nemail = \"{email}\"\nbase_url = \"{base_url}\"\n\n[jmap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"rt-old\"\nauthorize_endpoint = \"{base_url}/oauth/authorize\"\ntoken_endpoint = \"{base_url}/token\"\nredirect_uri = \"{REDIRECT_URI}\"\nscopes = [\"offline_access\"]\nresource = \"{base_url}/jmap/session\"\n"
    );
    load_str(&doc)
        .expect("a valid jmap document")
        .jmap
        .expect("the jmap section")
}
