// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the account's IMAP connections: the folders a dial binds and in what order,
//! the one login they share, and what a first-login refusal is read as.
//!
//! The server here is a small scripted IMAP responder over TLS that counts the connections it
//! accepts, because the connection is what a server with a per-user limit counts. It answers
//! every command generically and reports a folder list with role folders, an untagged folder and
//! a nested one, which is all the dial needs.
//!
//! In their own `#[path]` file to keep `imap.rs` under the 500-line limit.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use engine_api::{FailureClass, SyncScope};
use rustls::pki_types::PrivatePkcs8KeyDer;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};
use tokio_rustls::TlsAcceptor;

use crate::{load_str, AccountConfig, AccountError, CertificateException, ImapConnections};

use super::*;

/// The folder list the server reports. Archive is tagged `\All`, as some servers do for a real
/// folder, Starred is a `\Flagged` view of mail filed elsewhere, and two folders carry no role.
const LIST: &str = "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n\
                    * LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\n\
                    * LIST (\\HasNoChildren \\Drafts) \"/\" \"Drafts\"\r\n\
                    * LIST (\\HasNoChildren \\Trash) \"/\" \"Trash\"\r\n\
                    * LIST (\\HasNoChildren \\All) \"/\" \"Archive\"\r\n\
                    * LIST (\\HasNoChildren \\Flagged) \"/\" \"Starred\"\r\n\
                    * LIST (\\HasChildren) \"/\" \"Projects\"\r\n\
                    * LIST (\\HasNoChildren) \"/\" \"Projects/2026\"\r\n";

/// The reply to one tagged command line.
fn reply(line: &str) -> String {
    let mut words = line.split_whitespace();
    let tag = words.next().unwrap_or("*");
    let command = words.next().unwrap_or_default().to_ascii_uppercase();
    match command.as_str() {
        "CAPABILITY" => format!("* CAPABILITY IMAP4rev1 IDLE SPECIAL-USE\r\n{tag} OK done\r\n"),
        "LIST" => format!("{LIST}{tag} OK LIST done\r\n"),
        "EXAMINE" | "SELECT" => {
            format!("* 0 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n{tag} OK [READ-ONLY] done\r\n")
        }
        _ => format!("{tag} OK done\r\n"),
    }
}

/// A TLS server socket answering IMAP lines with `reply`, counting what it accepts.
async fn scripted_server() -> (
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

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local address").port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&accepted);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut tls = BufReader::new(tls);
                if tls.write_all(b"* OK ready\r\n").await.is_err() {
                    return;
                }
                let mut line = String::new();
                while matches!(tls.read_line(&mut line).await, Ok(read) if read > 0) {
                    let answer = reply(&line);
                    if tls.write_all(answer.as_bytes()).await.is_err() {
                        return;
                    }
                    line.clear();
                }
            });
        }
    });
    (port, der, accepted)
}

/// An account pointed at the server, trusting its certificate by exception.
fn account(port: u16, served: &rustls::pki_types::CertificateDer<'static>) -> AccountConfig {
    let exception = CertificateException::new("127.0.0.1", served);
    load_str(&format!(
        "[imap]\naddr = \"127.0.0.1:{port}\"\nserver_name = \"127.0.0.1\"\n\
         username = \"someone@example.com\"\npassword = \"secret\"\n\
         \n[[certificate_exception]]\nserver_name = \"{}\"\nsha256 = \"{}\"\n",
        exception.server_name, exception.sha256,
    ))
    .expect("a valid account config")
}

/// The same account with the SMTP submission leg: the endpoint rides the same dial, over
/// the same accepted certificate (both legs name the same server). Submission opens its own
/// connection per send, so the dial resolves the transport without opening one.
fn submission_account(
    port: u16,
    served: &rustls::pki_types::CertificateDer<'static>,
) -> AccountConfig {
    let exception = CertificateException::new("127.0.0.1", served);
    load_str(&format!(
        "[imap]\naddr = \"127.0.0.1:{port}\"\nserver_name = \"127.0.0.1\"\n\
         username = \"someone@example.com\"\npassword = \"secret\"\n\
         \n[smtp]\naddr = \"127.0.0.1:{port}\"\nserver_name = \"127.0.0.1\"\n\
         \n[[certificate_exception]]\nserver_name = \"{}\"\nsha256 = \"{}\"\n",
        exception.server_name, exception.sha256,
    ))
    .expect("a valid account config")
}

fn account_id() -> AccountId {
    AccountId::try_from("someone@127.0.0.1").expect("account id")
}

/// The mailbox each provider is bound to, in the order the dial returned them.
fn bound_mailboxes(providers: &[Box<dyn Provider>]) -> Vec<String> {
    providers
        .iter()
        .map(|provider| match provider.email_scope(&account_id()) {
            SyncScope::ImapMailbox { mailbox, .. } => mailbox.as_str().to_owned(),
            other => panic!("an IMAP folder provider bound to {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn a_dial_binds_every_folder_the_account_lists_with_the_inbox_first() {
    // A folder the dial leaves out is in no account pass and is watched by nothing, so its mail
    // arrives only when someone opens it. Role or none, nested or not, every listed folder is
    // bound, and the Inbox once, first; a view of mail filed elsewhere (Starred) is not.
    let (port, served, _accepted) = scripted_server().await;
    let config = account(port, &served);
    let connections = ImapConnections::new();

    let providers = connect_mail_providers(&connections, &config, &account_id(), None)
        .await
        .expect("the dial");

    assert_eq!(
        bound_mailboxes(&providers),
        [
            "INBOX",
            "Sent",
            "Drafts",
            "Trash",
            "Archive",
            "Projects",
            "Projects/2026",
        ],
    );
}

#[tokio::test]
async fn a_dial_binds_the_submission_provider_when_the_account_configures_smtp() {
    // The engine's outbox drain delivers through the account's own providers, so the SMTP
    // leg must ride the dial: every provider it returns carries the submission binding,
    // beside the mail binding, over the one login. This is the registered provider set the
    // drain is handed (its first provider), so submission missing here would strand every
    // queued send.
    let (port, served, _accepted) = scripted_server().await;
    let config = submission_account(port, &served);

    let providers = connect_mail_providers(&ImapConnections::new(), &config, &account_id(), None)
        .await
        .expect("the dial");

    assert!(!providers.is_empty(), "the dial bound the folders");
    for provider in &providers {
        let capabilities = provider.connection_info().capabilities;
        assert!(
            capabilities.submission(),
            "a bound provider advertises no submission: {capabilities:?}",
        );
        assert!(
            capabilities.mail(),
            "submission rode beside the mail binding"
        );
    }
}

#[tokio::test]
async fn a_dial_without_smtp_binds_mail_only_providers() {
    // The recorded shape for an account that configures no submission: every provider the
    // dial returns is mail only. Drafts stay bound (an APPEND needs no SMTP transport), so
    // the missing capability is submission alone, not the mail half of the set.
    let (port, served, _accepted) = scripted_server().await;
    let config = account(port, &served);

    let providers = connect_mail_providers(&ImapConnections::new(), &config, &account_id(), None)
        .await
        .expect("the dial");

    for provider in &providers {
        let capabilities = provider.connection_info().capabilities;
        assert!(
            !capabilities.submission(),
            "a mail-only account advertised submission: {capabilities:?}",
        );
        assert!(capabilities.mail());
        assert!(capabilities.mail_drafts());
    }
}

#[tokio::test]
async fn a_dial_binds_every_folder_over_one_login() {
    let (port, served, accepted) = scripted_server().await;
    let config = account(port, &served);
    let connections = ImapConnections::new();

    let providers = connect_mail_providers(&connections, &config, &account_id(), None)
        .await
        .expect("the dial");
    let opened = connect_imap_mailbox(&connections, &config, "Projects", None)
        .await
        .expect("a folder opened on demand");

    assert_eq!(providers.len(), 7, "every listed folder");
    drop(opened);
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "binding a folder opened a socket of its own",
    );
}

#[tokio::test]
async fn folders_opened_together_before_any_dial_connect_the_account_once() {
    let (port, served, accepted) = scripted_server().await;
    let config = account(port, &served);
    let connections = ImapConnections::new();

    let opened = futures::future::join_all(
        ["INBOX", "Sent", "Projects"]
            .map(|mailbox| connect_imap_mailbox(&connections, &config, mailbox, None)),
    )
    .await;

    assert!(opened.iter().all(Result::is_ok));
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "each caller that found no account connected one of its own",
    );
}

#[tokio::test]
async fn every_dial_logs_in_afresh() {
    // A dial's login is the one whose refusal can mean the password is wrong, so a dial never
    // rides an account a previous dial connected.
    let (port, served, accepted) = scripted_server().await;
    let config = account(port, &served);
    let connections = ImapConnections::new();

    for _ in 0..2 {
        connect_mail_providers(&connections, &config, &account_id(), None)
            .await
            .expect("the dial");
    }

    assert_eq!(accepted.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn invalidating_the_connections_makes_the_next_call_dial() {
    let (port, served, accepted) = scripted_server().await;
    let config = account(port, &served);
    let connections = ImapConnections::new();
    let providers = connect_mail_providers(&connections, &config, &account_id(), None)
        .await
        .expect("the dial");

    connections.invalidate();
    providers[0]
        .sync_mailboxes(&account_id(), None)
        .await
        .expect("the folder list, over a new connection");

    assert_eq!(accepted.load(Ordering::SeqCst), 2);
}

/// A server that greets, reads the first `LOGIN`, and refuses it with `refusal`: the one
/// refusal that can mean the stored password itself is no good.
async fn server_refusing_login_with(
    refusal: &'static str,
) -> (u16, rustls::pki_types::CertificateDer<'static>) {
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
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut tls = BufReader::new(tls);
                let _ = tls
                    .write_all(b"* OK [CAPABILITY IMAP4rev1] ready\r\n")
                    .await;
                let mut login = String::new();
                if tls.read_line(&mut login).await.is_err() {
                    return;
                }
                let tag = login.split_whitespace().next().unwrap_or("*");
                let _ = tls
                    .write_all(format!("{tag} {refusal}\r\n").as_bytes())
                    .await;
            });
        }
    });
    (port, der)
}

/// The dial's first login, against a server that refuses every one.
async fn first_login(config: &AccountConfig) -> AccountError {
    match connect_mail_providers(&ImapConnections::new(), config, &account_id(), None).await {
        Ok(_) => panic!("the server refuses every LOGIN"),
        Err(err) => err,
    }
}

/// A limit refusal, read as a rejected sign-in, would tell the user their password is wrong
/// and ask for it again, which cannot help: it stays an IMAP failure in its own class.
#[tokio::test]
async fn a_first_login_refused_for_a_limit_is_not_a_rejected_sign_in() {
    let (port, served) = server_refusing_login_with("NO [LIMIT] LOGIN Rate limit hit.").await;
    let config = account(port, &served);

    let error = first_login(&config).await;
    let AccountError::Imap(imap) = &error else {
        panic!("a limit was read as something other than an IMAP failure: {error}");
    };
    assert_eq!(imap.failure_class(), FailureClass::RateLimited, "{error}");
}

/// The other direction, so the test above cannot pass because the fake failed before `LOGIN`:
/// the same server refusing the credential still raises the sign-in prompt.
#[tokio::test]
async fn a_first_login_refused_for_the_credential_is_a_rejected_sign_in() {
    let (port, served) =
        server_refusing_login_with("NO [AUTHENTICATIONFAILED] Authentication failed.").await;
    let config = account(port, &served);

    let error = first_login(&config).await;
    assert!(
        matches!(error, AccountError::SigninRejected(_)),
        "a refused credential was not read as one: {error}"
    );
}
