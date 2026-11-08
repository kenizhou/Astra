// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the account watch wrapper: the IMAP INBOX watch an idle
//! server stands, the poll answers a server without a push face gives, the
//! headroom guard, and the JMAP EventSource watch the stored section's
//! server opens.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use rustls::pki_types::PrivatePkcs8KeyDer;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener as AsyncListener,
};
use tokio_rustls::TlsAcceptor;

use super::{open_watch, WatchFace};
use crate::{load_str, AccountConfig, CertificateException, ImapConnections};

/// An IMAP account pointed at the scripted server, trusting its certificate
/// by exception, the shape every IMAP test here dials.
fn imap_account(port: u16, served: &rustls::pki_types::CertificateDer<'static>) -> AccountConfig {
    let exception = CertificateException::new("127.0.0.1", served);
    load_str(&format!(
        "[imap]\naddr = \"127.0.0.1:{port}\"\nserver_name = \"127.0.0.1\"\n\
         username = \"someone@example.com\"\npassword = \"secret\"\n\
         \n[[certificate_exception]]\nserver_name = \"{}\"\nsha256 = \"{}\"\n",
        exception.server_name, exception.sha256,
    ))
    .expect("a valid account config")
}

/// A scripted TLS IMAP server that greets with `IDLE` in its capabilities
/// when `idle`, answers the login and the INBOX examine, and on `IDLE`
/// sends the continuation, one untagged `EXISTS`, and the completion for
/// the `DONE` that follows: the whole exchange a standing watch runs.
async fn idle_server(
    idle: bool,
) -> (
    u16,
    rustls::pki_types::CertificateDer<'static>,
    Arc<AtomicUsize>,
) {
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

    let listener = AsyncListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local address").port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&accepted);
    let caps = if idle { "IMAP4rev1 IDLE" } else { "IMAP4rev1" };
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut tls = BufReader::new(tls);
                if tls
                    .write_all(format!("* OK [CAPABILITY {caps}] ready\r\n").as_bytes())
                    .await
                    .is_err()
                {
                    return;
                }
                let mut line = String::new();
                while matches!(tls.read_line(&mut line).await, Ok(read) if read > 0) {
                    let mut words = line.split_whitespace();
                    let tag = words.next().unwrap_or("*").to_owned();
                    let command = words.next().unwrap_or_default().to_ascii_uppercase();
                    match command.as_str() {
                        "IDLE" => {
                            if tls.write_all(b"+ idling\r\n").await.is_err() {
                                return;
                            }
                            // The delivery-critical push: one message arrives
                            // while the watch stands.
                            if tls.write_all(b"* 1 EXISTS\r\n").await.is_err() {
                                return;
                            }
                            let mut done = String::new();
                            let _ = tls.read_line(&mut done).await;
                            if tls
                                .write_all(format!("{tag} OK done\r\n").as_bytes())
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        "EXAMINE" | "SELECT" => {
                            let answer = format!(
                                "* 0 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n\
                                 {tag} OK [READ-ONLY] done\r\n"
                            );
                            if tls.write_all(answer.as_bytes()).await.is_err() {
                                return;
                            }
                        }
                        // The dial settles its dialect and extension set on
                        // the answer to `CAPABILITY`, so the watch's
                        // precondition is negotiated here, not assumed from
                        // the greeting.
                        "CAPABILITY" if idle => {
                            let answer =
                                format!("* CAPABILITY IMAP4rev1 IDLE\r\n{tag} OK done\r\n");
                            if tls.write_all(answer.as_bytes()).await.is_err() {
                                return;
                            }
                        }
                        "LOGOUT" => {
                            let _ = tls
                                .write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())
                                .await;
                            return;
                        }
                        _ => {
                            if tls
                                .write_all(format!("{tag} OK done\r\n").as_bytes())
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    line.clear();
                }
            });
        }
    });
    (port, der, accepted)
}

#[tokio::test]
async fn an_idle_server_opens_an_inbox_watch_that_reports_the_change() {
    let (port, served, _accepted) = idle_server(true).await;
    let connections = ImapConnections::new();
    let face = open_watch(&connections, &imap_account(port, &served))
        .await
        .expect("the watch opens");

    let mut watch = match face {
        WatchFace::Watch(watch) => watch,
        WatchFace::Poll => panic!("an idle-capable server answers with a watch"),
    };
    // The pushed EXISTS lands as a Changed: the whole contract the loop
    // drives, one event, no data, the pass is the reconciliation.
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), watch.next())
        .await
        .expect("the watch answers")
        .expect("the event reads");
    assert_eq!(event, engine_provider::WatchEvent::Changed);
}

#[tokio::test]
async fn a_server_without_idle_answers_poll() {
    // No IDLE in the capabilities, so there is no watch to stand: the
    // account's cadence carries it, and the loop never re-asks a server
    // that already answered.
    let (port, served, _accepted) = idle_server(false).await;
    let face = open_watch(&ImapConnections::new(), &imap_account(port, &served))
        .await
        .expect("the answer reads");
    assert!(matches!(face, WatchFace::Poll));
}

#[tokio::test]
async fn the_headroom_guard_leaves_the_last_connection_for_syncing() {
    // Watches are standing connections; the engine refuses the one that
    // would leave the account nothing to sync with, and the wrapper hands
    // that refusal back as Poll rather than an error the loop would retry.
    let (port, served, _accepted) = idle_server(true).await;
    let connections = ImapConnections::new();
    let account = imap_account(port, &served);

    let mut standing = Vec::new();
    loop {
        let face = open_watch(&connections, &account)
            .await
            .expect("the answer");
        let WatchFace::Watch(watch) = face else {
            break;
        };
        standing.push(watch);
        assert!(
            standing.len() <= 4,
            "the guard never took the last connection"
        );
    }
    assert!(
        !standing.is_empty(),
        "an idle account with headroom stands its watch"
    );
}

#[tokio::test]
async fn a_graph_account_answers_poll_without_dialling() {
    // The OAuth families have no push channel to stand, so the wrapper
    // answers Poll from the stored document alone; a dial here would be a
    // network round trip a poll answer never needs.
    let config = load_str(
        "[graph]\nemail = \"me@contoso.test\"\nclient_id = \"cid\"\ntenant = \"common\"\n\
         redirect_uri = \"http://127.0.0.1:1/cb\"\nscopes = []\nrefresh_token = \"rt\"\n",
    )
    .expect("a valid graph document");
    let face = open_watch(&ImapConnections::new(), &config)
        .await
        .expect("the answer reads");
    assert!(matches!(face, WatchFace::Poll));
}

/// A scripted JMAP server: the session document at the well-known path
/// (advertising an `eventSourceUrl` when `events` names one) and a
/// `text/event-stream` reply carrying one mail state change everywhere
/// else. Returns its base URL.
fn jmap_server(events: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let request = String::from_utf8_lossy(&buf);
            let reply = if request.starts_with("GET /.well-known/jmap") {
                let mail = "urn:ietf:params:jmap:mail";
                let source = if events {
                    format!(
                        "\"eventSourceUrl\":\"http://{addr}/events?types={{types}}\
                         &closeafter={{closeafter}}&ping={{ping}}\","
                    )
                } else {
                    String::new()
                };
                format!(
                    "{{\"apiUrl\":\"http://{addr}/api\",{source}\
                     \"capabilities\":{{\"urn:ietf:params:jmap:core\":{{}},\"{mail}\":{{}}}},\
                     \"primaryAccounts\":{{\"{mail}\":\"A1\"}},\
                     \"accounts\":{{\"A1\":{{\"name\":\"scripted\",\
                     \"accountCapabilities\":{{\"{mail}\":{{}}}}}}}}}}"
                )
            } else {
                // One mail StateChange, then the stream closes: enough for
                // the watch to open and report its first Changed.
                "event: state\r\ndata: {\"@type\":\"StateChange\",\
                 \"changed\":{\"A1\":{\"Email\":\"2\"}}}\r\n\r\n"
                    .to_owned()
            };
            let content_type = if request.starts_with("GET /.well-known/jmap") {
                "application/json"
            } else {
                "text/event-stream"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

/// A jmap password account dialling `base_url`.
fn jmap_account_at(base_url: &str) -> AccountConfig {
    load_str(&format!(
        "[jmap]\nemail = \"me@example.net\"\nbase_url = \"{base_url}\"\npassword = \"pw\"\n"
    ))
    .expect("a valid jmap document")
}

#[tokio::test]
async fn a_jmap_account_opens_its_event_source_watch() {
    let base = jmap_server(true);
    let face = open_watch(&ImapConnections::new(), &jmap_account_at(&base))
        .await
        .expect("the watch opens");
    let mut watch = match face {
        WatchFace::Watch(watch) => watch,
        WatchFace::Poll => panic!("a server with an event source answers with a watch"),
    };
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), watch.next())
        .await
        .expect("the watch answers")
        .expect("the event reads");
    assert_eq!(event, engine_provider::WatchEvent::Changed);
}

#[tokio::test]
async fn a_jmap_server_with_no_event_source_url_answers_poll() {
    let base = jmap_server(false);
    let face = open_watch(&ImapConnections::new(), &jmap_account_at(&base))
        .await
        .expect("the answer reads");
    assert!(matches!(face, WatchFace::Poll));
}
