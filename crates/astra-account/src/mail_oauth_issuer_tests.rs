// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the issuer chain: the candidate order, the registration
//! gate, the fail-soft, the registrable-domain helper, and the
//! session-scoped registration cache.

use astra_autodetect::{
    AuthKind, DetectedDav, DetectedMailSettings, DetectedServer, SocketKind, Source, SourceKind,
};

use super::*;
use crate::jmap::test_support::{discovery_server, full_chain, metadata, Answer};

/// A detection result whose `oauth_issuer` is `issuer`: the field the
/// autodetect crate already parsed and trust-filtered upstream.
fn detected_with(issuer: Option<String>) -> DetectedMailSettings {
    DetectedMailSettings {
        incoming: vec![DetectedServer {
            hostname: "mail.example.net".to_owned(),
            port: 993,
            socket: SocketKind::Tls,
            auth: vec![AuthKind::OAuth2],
            username: "you@example.net".to_owned(),
        }],
        outgoing: Vec::new(),
        is_trusted: true,
        source: Source {
            kind: SourceKind::Autoconfig,
            url: "https://autoconfig.example.net/mail/config-v1.1.xml".to_owned(),
        },
        oauth_issuer: issuer,
        dav: DetectedDav::default(),
    }
}

/// A base URL nothing listens on, so any candidate pointed at it fails
/// before a byte leaves the machine: the offline tests' stand-in for a
/// domain that publishes nothing.
fn refused_base() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    drop(listener);
    format!("http://{addr}")
}

/// An email domain the tests own: a loopback address whose port nothing
/// listens on, so the well-known candidates fail offline.
fn dead_email_domain() -> String {
    refused_base().trim_start_matches("http://").to_owned()
}

#[tokio::test]
async fn the_autoconfig_issuer_is_the_first_candidate() {
    let (base, seen) = discovery_server(full_chain);
    let found = mail_oauth_issuer(
        &dead_email_domain(),
        Some(&detected_with(Some(base.clone()))),
    )
    .await
    .expect("the provider's own issuer is found");
    assert_eq!(found.issuer, base);
    assert!(
        found.registration_endpoint.is_some(),
        "the gate needs open registration"
    );
    assert!(
        seen.lock()
            .expect("seen")
            .iter()
            .any(|path| path == "/.well-known/oauth-authorization-server"),
        "the well-known document was read"
    );
}

#[tokio::test]
async fn a_metadata_without_open_registration_is_not_an_offer() {
    // The server publishes metadata but no registration endpoint: it
    // admits only applications it registered in advance, and this build
    // holds no registration, so the answer is None (the
    // RegistrationNeeded screen) rather than a half-offer.
    let (base, _seen) = discovery_server(|b, path, method| match (method, path) {
        ("GET", "/.well-known/oauth-authorization-server") => {
            Some(("200 OK", String::new(), metadata(b, false)))
        }
        _ => full_chain(b, path, method),
    });
    assert!(
        mail_oauth_issuer(&dead_email_domain(), Some(&detected_with(Some(base))))
            .await
            .is_none(),
        "no registration endpoint means no offer"
    );
}

#[test]
fn the_well_known_candidates_follow_the_autoconfig_route() {
    // The autoconfig issuer as parsed, then the email domain, then its
    // registrable domain, deduplicated: a domain that already is its
    // registrable domain is probed once.
    let detected = detected_with(Some("https://id.example.net".to_owned()));
    assert_eq!(
        issuer_candidates("mail.example.co.uk", Some(&detected)),
        [
            "https://id.example.net".to_owned(),
            "https://mail.example.co.uk".to_owned(),
            "https://example.co.uk".to_owned(),
        ]
    );
    assert_eq!(
        issuer_candidates("example.net", Some(&detected)),
        [
            "https://id.example.net".to_owned(),
            "https://example.net".to_owned(),
        ]
    );
    // No detection, and a domain the list cannot cut: the domain alone,
    // and then nothing.
    assert_eq!(
        issuer_candidates("example.net", None),
        ["https://example.net".to_owned()]
    );
    assert_eq!(issuer_candidates("", None), [] as [std::string::String; 0]);
    // An issuer the detection already carried needs no domain at all.
    let detected_only = detected_with(Some("https://id.example.net".to_owned()));
    assert_eq!(
        issuer_candidates("", Some(&detected_only)),
        ["https://id.example.net".to_owned()]
    );
}

#[tokio::test]
async fn every_miss_resolves_to_none() {
    // No autoconfig issuer, and a domain nothing answers: the question
    // went unanswered, which is the password field, never an error.
    assert!(
        mail_oauth_issuer(&dead_email_domain(), Some(&detected_with(None)))
            .await
            .is_none()
    );
    assert!(mail_oauth_issuer(&dead_email_domain(), None)
        .await
        .is_none());
}

#[tokio::test]
async fn an_untrusted_domain_string_is_not_probed() {
    // The well-known candidates are HTTPS URLs of the domain; anything
    // that cannot form one is skipped rather than sent.
    assert!(mail_oauth_issuer("not a domain", None).await.is_none());
    assert!(mail_oauth_issuer("", None).await.is_none());
}

#[test]
fn the_registrable_domain_cut_is_public_suffix_aware() {
    // A multi-label public suffix: the naive last-two-labels cut would
    // answer `co.uk`, which is a public suffix itself.
    assert_eq!(
        registrable_domain("mail.example.co.uk").as_deref(),
        Some("example.co.uk")
    );
    assert_eq!(
        registrable_domain("a.b.example.com").as_deref(),
        Some("example.com")
    );
    // A domain that already is its registrable domain.
    assert_eq!(
        registrable_domain("example.net").as_deref(),
        Some("example.net")
    );
    // Nothing the list cannot answer.
    assert_eq!(registrable_domain("localhost"), None);
    assert_eq!(registrable_domain(""), None);
    assert_eq!(registrable_domain("127.0.0.1"), None);
}

#[tokio::test]
async fn the_registration_cache_reuses_the_id_minted_this_session() {
    let (base, seen) = discovery_server(full_chain);
    let found = mail_oauth_issuer(&dead_email_domain(), Some(&detected_with(Some(base))))
        .await
        .expect("the issuer");
    let redirect = "com.kenizhou.astra://mail-oauth";
    let first = register_mail_client(&found, redirect)
        .await
        .expect("registered");
    assert_eq!(first.client_id, "client-xyz");
    // The second attempt for the same issuer and redirect reuses it:
    // dynamic registration mints a new client every call, and a flaky
    // sign-in must not leave orphaned registrations behind.
    let second = register_mail_client(&found, redirect)
        .await
        .expect("reused");
    assert_eq!(second.client_id, first.client_id);
    let registrations = seen
        .lock()
        .expect("seen")
        .iter()
        .filter(|path| *path == "/register")
        .count();
    assert_eq!(registrations, 1, "one registration served two attempts");
}

/// The chain with a registration endpoint that issues a distinct client
/// id per call, so a test can tell one registration from two.
fn counting_chain(base: &str, path: &str, method: &str) -> Answer {
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    match (method, path) {
        ("GET", "/.well-known/oauth-authorization-server") => {
            Some(("200 OK", String::new(), metadata(base, true)))
        }
        ("POST", "/register") => {
            let issued = CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some((
                "201 Created",
                String::new(),
                format!(r#"{{"client_id":"client-{issued}"}}"#),
            ))
        }
        _ => None,
    }
}

#[tokio::test]
async fn a_different_redirect_registers_its_own_client() {
    let (base, seen) = discovery_server(counting_chain);
    let found = mail_oauth_issuer(&dead_email_domain(), Some(&detected_with(Some(base))))
        .await
        .expect("the issuer");
    let first = register_mail_client(&found, "com.kenizhou.astra://one")
        .await
        .expect("registered");
    let second = register_mail_client(&found, "com.kenizhou.astra://two")
        .await
        .expect("registered");
    assert_ne!(first.client_id, second.client_id);
    let registrations = seen
        .lock()
        .expect("seen")
        .iter()
        .filter(|path| *path == "/register")
        .count();
    assert_eq!(registrations, 2, "two redirects, two registrations");
}
