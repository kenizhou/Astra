// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the discovery chain's wiring to the JMAP session's own
//! `401`: the offer advertised only where the server advertises
//! registration, the fail-soft that leaves every miss as "no sign-in
//! here", the authorization URL built from the discovered endpoints, and
//! the one-registration-per-session rule.

use super::super::jmap_signin_offered;
use crate::jmap::oauth::begin_jmap_login;
use crate::jmap::oauth::test_support::{
    discovery_server, full_chain, metadata, no_mail_chain, refused_base, Answer, REDIRECT_URI,
};

#[tokio::test]
async fn signin_is_offered_where_the_server_advertises_registration() {
    let (base, seen) = discovery_server(full_chain);
    assert!(
        jmap_signin_offered("me@example.net", Some(&base)).await,
        "a server advertising the whole chain offers sign-in",
    );
    // And the probe costs no registration: it exists to decide whether to
    // show the button, not to mint a client.
    assert!(
        !seen
            .lock()
            .expect("seen lock")
            .iter()
            .any(|path| path == "/register"),
        "the probe registered a client",
    );
}

/// The same chain with a null registration_endpoint: a server that
/// offers no open registration.
fn withdrawn(base: &str, path: &str, method: &str) -> Answer {
    match (method, path) {
        ("GET", "/.well-known/jmap") => full_chain(base, path, method),
        ("GET", "/.well-known/oauth-protected-resource") => Some((
            "200 OK",
            String::new(),
            format!("{{\"authorization_servers\":[\"{base}\"]}}"),
        )),
        ("GET", "/.well-known/oauth-authorization-server") => {
            Some(("200 OK", String::new(), metadata(base, false)))
        }
        _ => None,
    }
}

#[tokio::test]
async fn a_server_withdrawing_registration_offers_nothing() {
    // There is no client id to be had from a null registration_endpoint, so
    // the button would dead-end in front of the password field that always
    // works.
    let (base, _seen) = discovery_server(withdrawn);
    assert!(
        !jmap_signin_offered("me@example.net", Some(&base)).await,
        "a server with no open registration does not offer sign-in",
    );
}

/// A server whose `401` challenges without naming any metadata: the chain
/// has no pointer and nothing to fall back to that answers.
fn silent(_base: &str, path: &str, method: &str) -> Answer {
    match (method, path) {
        ("GET", "/.well-known/jmap") => Some((
            "401 Unauthorized",
            "WWW-Authenticate: Basic realm=\"jmap\"\r\n".to_owned(),
            "{}".to_owned(),
        )),
        _ => None,
    }
}

#[tokio::test]
async fn the_probe_fails_soft_on_every_miss() {
    // Nothing listening, and a bare `401` with no pointer: each is "no
    // sign-in here", never an error to surface over the password field.
    assert!(
        !jmap_signin_offered("me@example.net", Some(&refused_base())).await,
        "an unreachable server is not an offer",
    );
    let (base, _seen) = discovery_server(silent);
    assert!(
        !jmap_signin_offered("me@example.net", Some(&base)).await,
        "a server naming no authorization server is not an offer",
    );
}

#[tokio::test]
async fn begin_builds_the_authorization_from_the_discovered_endpoints() {
    let (base, _seen) = discovery_server(full_chain);
    let start = begin_jmap_login("me@example.net", Some(&base), REDIRECT_URI)
        .await
        .expect("a sign-in to begin");
    let url = &start.authorization_url;
    assert!(
        url.starts_with(&format!("{base}/oauth/authorize?")),
        "the discovered authorize endpoint is where the browser goes: {url}",
    );
    assert!(url.contains("client_id=client-xyz"), "{url}");
    assert!(url.contains("code_challenge_method=S256"), "{url}");
    assert!(url.contains("login_hint=me%40example.net"), "{url}");
    // RFC 8707: the resource the metadata published rides the request.
    assert!(url.contains("resource=http%3A%2F%2F127.0.0.1%3A"), "{url}");
    assert!(url.contains("%2Fjmap%2Fsession"), "{url}");
    // The scopes are limited to the capabilities the app uses: the admin
    // scope the server also advertises is not asked for.
    assert!(url.contains("offline_access"), "{url}");
    assert!(
        url.contains("urn%3Aietf%3Aparams%3Aoauth%3Ascope%3Amail"),
        "{url}",
    );
    assert!(!url.contains("admin"), "{url}");
}

#[tokio::test]
async fn begin_registers_once_per_issuer_and_redirect() {
    // Dynamic registration mints a NEW client id on every call, so a
    // cancelled or failed attempt must not leave another orphaned
    // registration behind: the second begin against the same server reuses
    // the first client.
    let (base, seen) = discovery_server(full_chain);
    let _ = begin_jmap_login("me@example.net", Some(&base), REDIRECT_URI)
        .await
        .expect("the first begin");
    let _ = begin_jmap_login("me@example.net", Some(&base), REDIRECT_URI)
        .await
        .expect("the second begin");
    let registrations = seen
        .lock()
        .expect("seen lock")
        .iter()
        .filter(|path| path.as_str() == "/register")
        .count();
    assert_eq!(registrations, 1, "a second client id was minted");
}

#[tokio::test]
async fn a_server_whose_signin_grants_no_mail_is_refused() {
    let (base, _seen) = discovery_server(no_mail_chain);
    let err = begin_jmap_login("me@example.net", Some(&base), REDIRECT_URI)
        .await
        .expect_err("a sign-in that grants no mail");
    assert!(
        err.to_string().contains("mail"),
        "the refusal must say it is about mail: {err}",
    );
}
