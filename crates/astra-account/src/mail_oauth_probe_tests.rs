// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the probe wrapper: the engine's real pre-auth probe against
//! a scripted TLS IMAP server, the three answers over the observed
//! capability lines, and the never-fail contract.

use std::sync::Arc;

use astra_autodetect::{
    AuthKind, DetectedDav, DetectedMailSettings, DetectedServer, SocketKind, Source, SourceKind,
};
use rustls::pki_types::PrivatePkcs8KeyDer;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};
use tokio_rustls::TlsAcceptor;

use super::*;
use crate::jmap::test_support::{discovery_server, full_chain};
use crate::{CertificateException, ConnectionSecurity};

/// The capability line a Stalwart server was observed to send before
/// authentication: password and OAuth mechanisms side by side.
const STALWART_LINE: &str = "IMAP4rev1 AUTH=PLAIN AUTH=OAUTHBEARER AUTH=XOAUTH2";

/// A TLS IMAP server whose pre-auth capability line is `line`: what the
/// engine's probe reads, over a self-signed certificate the test's trust
/// config carries an exception for.
async fn capability_server(line: &str) -> (u16, rustls::pki_types::CertificateDer<'static>) {
    let key = rcgen::KeyPair::generate().expect("key pair");
    let params =
        rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()]).expect("certificate params");
    let certificate = params.self_signed(&key).expect("self-signed certificate");
    let der = certificate.der().clone();
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("protocol versions")
    .with_no_client_auth()
    .with_single_cert(
        vec![der.clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
    .expect("server certificate and key");
    let acceptor = TlsAcceptor::from(Arc::new(server));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local address").port();
    let line = line.to_owned();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let line = line.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut tls = BufReader::new(tls);
                if tls.write_all(b"* OK ready\r\n").await.is_err() {
                    return;
                }
                let mut command = String::new();
                while matches!(tls.read_line(&mut command).await, Ok(read) if read > 0) {
                    let tag = command.split_whitespace().next().unwrap_or("a1");
                    let answer = if command.contains("CAPABILITY") {
                        format!("* CAPABILITY {line}\r\n{tag} OK done\r\n")
                    } else if command.contains("LOGOUT") {
                        format!("* BYE\r\n{tag} OK LOGOUT\r\n")
                    } else {
                        format!("{tag} OK done\r\n")
                    };
                    if tls.write_all(answer.as_bytes()).await.is_err() {
                        return;
                    }
                    command.clear();
                }
            });
        }
    });
    (port, der)
}

/// The trust config the probe runs under for a scripted server: the
/// setup policy with the server's certificate accepted, exactly the
/// wizard's TLS acceptance walk.
fn accepted_tls(
    served: &rustls::pki_types::CertificateDer<'static>,
) -> engine_tls::TlsClientConfig {
    let exception = CertificateException::new("127.0.0.1", served);
    let engine = exception.to_engine().expect("a readable fingerprint");
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[engine])
        .expect("a trust config")
}

/// A detection result whose `oauth_issuer` is `issuer`.
fn detected_with(issuer: Option<String>) -> DetectedMailSettings {
    DetectedMailSettings {
        incoming: vec![DetectedServer {
            hostname: "127.0.0.1".to_owned(),
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

/// A base URL nothing listens on: the offline tests' stand-in for a
/// domain that publishes no issuer.
fn refused_base() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    drop(listener);
    format!("http://{addr}")
}

/// The query for a scripted server, its email domain a loopback address
/// nothing answers, so the chain's fall-through candidates stay offline
/// and the one live candidate is the detection's issuer.
fn query(port: u16, detected: Option<&DetectedMailSettings>) -> ImapAuthQuery<'_> {
    let dead = refused_base();
    ImapAuthQuery {
        imap_host: format!("127.0.0.1:{port}"),
        imap_security: ConnectionSecurity::ImplicitTls,
        email: format!("you@{}", dead.trim_start_matches("http://")),
        detected,
    }
}

#[tokio::test]
async fn the_observed_stalwart_line_with_a_live_issuer_offers_sign_in() {
    let (auth_base, _seen) = discovery_server(full_chain);
    let detected = detected_with(Some(auth_base.clone()));
    let (port, served) = capability_server(STALWART_LINE).await;
    let answer = imap_auth_offer(&query(port, Some(&detected)), &accepted_tls(&served)).await;
    assert_eq!(
        answer,
        ImapAuth::SignIn {
            issuer: auth_base,
            provider_label: None,
            password_also_works: true,
        },
        "the server takes a token, a route was found, and the password line stands beside it"
    );
}

#[tokio::test]
async fn an_oauth_server_with_no_route_answers_registration_needed() {
    // The server takes a token, but no authorization server answers for
    // the domain: the honest middle answer, not a bare password form.
    let (port, served) = capability_server(STALWART_LINE).await;
    let detected = detected_with(None);
    let answer = imap_auth_offer(&query(port, Some(&detected)), &accepted_tls(&served)).await;
    assert_eq!(
        answer,
        ImapAuth::RegistrationNeeded {
            password_also_works: true,
        }
    );
}

#[tokio::test]
async fn a_server_that_admits_no_password_offers_sign_in_without_the_fallback() {
    // The Microsoft shape: OAuth only, the LOGIN command withdrawn, no
    // password mechanism. The password field would be a dead end, and
    // the answer says so.
    let (auth_base, _seen) = discovery_server(full_chain);
    let detected = detected_with(Some(auth_base.clone()));
    let (port, served) = capability_server("IMAP4rev1 AUTH=OAUTHBEARER LOGINDISABLED").await;
    let answer = imap_auth_offer(&query(port, Some(&detected)), &accepted_tls(&served)).await;
    assert_eq!(
        answer,
        ImapAuth::SignIn {
            issuer: auth_base,
            provider_label: None,
            password_also_works: false,
        }
    );
}

#[tokio::test]
async fn a_password_only_server_answers_password() {
    let (port, served) = capability_server("IMAP4rev1 AUTH=PLAIN AUTH=LOGIN").await;
    let detected = detected_with(None);
    let answer = imap_auth_offer(&query(port, Some(&detected)), &accepted_tls(&served)).await;
    assert_eq!(answer, ImapAuth::Password);
}

#[tokio::test]
async fn an_unanswered_probe_is_the_password_form() {
    // Nothing listens at the dial address: the decision never fails, and
    // the answer that works everywhere stands.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    let (served_port, served) = capability_server(STALWART_LINE).await;
    let _ = served_port;
    let detected = detected_with(None);
    let answer = imap_auth_offer(&query(port, Some(&detected)), &accepted_tls(&served)).await;
    assert_eq!(answer, ImapAuth::Password);
}

#[test]
fn the_route_decides_the_three_answers_over_the_servers_own_word() {
    // The table ships empty, and the classification honours whatever it
    // holds: a table route wins with the provider's own label, an
    // issuer route wins without one, and neither leaves the OAuth
    // question unanswered.
    assert_eq!(
        classify(
            true,
            true,
            &Route::Table("Example Mail", "https://id.example.net")
        ),
        ImapAuth::SignIn {
            issuer: "https://id.example.net".to_owned(),
            provider_label: Some("Example Mail".to_owned()),
            password_also_works: true,
        }
    );
    assert_eq!(
        classify(
            true,
            true,
            &Route::Issuer("https://id.example.net".to_owned())
        ),
        ImapAuth::SignIn {
            issuer: "https://id.example.net".to_owned(),
            provider_label: None,
            password_also_works: true,
        }
    );
    assert_eq!(
        classify(true, true, &Route::None),
        ImapAuth::RegistrationNeeded {
            password_also_works: true,
        }
    );
    // No OAuth mechanism: the password form regardless of any route, and
    // a route cannot conjure a password onto a server that refuses one.
    assert_eq!(
        classify(
            false,
            true,
            &Route::Issuer("https://id.example.net".to_owned())
        ),
        ImapAuth::Password
    );
    assert_eq!(
        classify(true, false, &Route::None),
        ImapAuth::RegistrationNeeded {
            password_also_works: false,
        }
    );
}
