// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the bypass transport: scripted SMTP and IMAP fixtures over
//! real TLS (a self-signed fixture cert, an accept-any client connector).

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use super::*;

/// Build the client connector that trusts anything (fixtures only) and the
/// server acceptor with a fresh self-signed cert.
fn tls_pair() -> (TlsConnector, TlsAcceptor) {
    let built = astra_crypto_smime::testing::build_self_signed_ca("CN=Transport Fixture CA");
    let client = engine_tls::TlsClientConfig::dangerous_accept_any().connector();
    let server_config = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(built.cert_der)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(built.priv_pkcs8_der)),
        )
        .expect("fixture server config");
    (client, TlsAcceptor::from(Arc::new(server_config)))
}

fn dial_for(port: u16, security: TransportSecurity) -> SmtpDial {
    SmtpDial {
        addr: format!("127.0.0.1:{port}"),
        server_name: "localhost".to_string(),
        security,
        username: "u@x.com".to_string(),
        password: "pw".to_string(),
    }
}

#[derive(Clone)]
struct Script {
    body: Arc<Mutex<Vec<u8>>>,
    close_after_354: bool,
    rcpt_code: u16,
    mail_code: u16,
    starttls: bool,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            body: Arc::new(Mutex::new(Vec::new())),
            close_after_354: false,
            rcpt_code: 250,
            mail_code: 250,
            starttls: false,
        }
    }
}

/// Run one scripted SMTP session on `stream` (already TLS-wrapped when
/// implicit; plaintext awaiting STARTTLS when the script says so).
async fn serve_smtp<S>(stream: S, script: Arc<Script>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut reader = BufReader::new(stream);
    reader.write_all(b"220 fixture ESMTP\r\n").await?;
    reader.flush().await?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let verb = line.trim_end().to_ascii_uppercase();
        if verb.starts_with("STARTTLS") {
            reader.write_all(b"220 go ahead\r\n").await?;
            reader.flush().await?;
            let plain = reader.into_inner();
            let (_, acceptor) = tls_pair();
            let tls = acceptor.accept(plain).await?;
            // Continue on the upgraded stream: same script, the greeting
            // was already read in plaintext on both sides.
            return serve_smtp_upgraded(tls, script).await;
        }
        if verb.starts_with("EHLO") {
            reader
                .write_all(b"250-fixture\r\n250 AUTH PLAIN\r\n")
                .await?;
        } else if verb.starts_with("AUTH") {
            reader.write_all(b"235 ok\r\n").await?;
        } else if verb.starts_with("MAIL") {
            let code = script.mail_code;
            reader
                .write_all(format!("{code} mail\r\n").as_bytes())
                .await?;
            if code / 100 != 2 {
                return Ok(());
            }
        } else if verb.starts_with("RCPT") {
            let code = script.rcpt_code;
            reader
                .write_all(format!("{code} rcpt\r\n").as_bytes())
                .await?;
            if code / 100 != 2 {
                return Ok(());
            }
        } else if verb.starts_with("DATA") {
            reader.write_all(b"354 go\r\n").await?;
            if script.close_after_354 {
                return Ok(());
            }
            // Read until the terminator line.
            let mut body = Vec::new();
            loop {
                let mut l = String::new();
                if reader.read_line(&mut l).await? == 0 {
                    return Ok(());
                }
                if l == ".\r\n" {
                    break;
                }
                body.extend_from_slice(l.as_bytes());
            }
            *script.body.lock().unwrap() = body;
            reader.write_all(b"250 queued\r\n").await?;
        } else if verb.starts_with("RSET") {
            reader.write_all(b"250 ok\r\n").await?;
        } else if verb.starts_with("QUIT") {
            reader.write_all(b"221 bye\r\n").await?;
            reader.flush().await?;
            return Ok(());
        } else {
            reader.write_all(b"502 huh\r\n").await?;
        }
        reader.flush().await?;
    }
}

/// The post-STARTTLS half: identical script, greeting skipped (already read
/// in plaintext).
async fn serve_smtp_upgraded<S>(stream: S, script: Arc<Script>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let verb = line.trim_end().to_ascii_uppercase();
        if verb.starts_with("EHLO") {
            reader
                .write_all(b"250-fixture\r\n250 AUTH PLAIN\r\n")
                .await?;
        } else if verb.starts_with("AUTH") {
            reader.write_all(b"235 ok\r\n").await?;
        } else if verb.starts_with("MAIL") {
            reader.write_all(b"250 mail\r\n").await?;
        } else if verb.starts_with("RCPT") {
            reader.write_all(b"250 rcpt\r\n").await?;
        } else if verb.starts_with("DATA") {
            reader.write_all(b"354 go\r\n").await?;
            let mut body = Vec::new();
            loop {
                let mut l = String::new();
                if reader.read_line(&mut l).await? == 0 {
                    return Ok(());
                }
                if l == ".\r\n" {
                    break;
                }
                body.extend_from_slice(l.as_bytes());
            }
            *script.body.lock().unwrap() = body;
            reader.write_all(b"250 queued\r\n").await?;
        } else if verb.starts_with("QUIT") {
            reader.write_all(b"221 bye\r\n").await?;
            reader.flush().await?;
            return Ok(());
        } else {
            reader.write_all(b"502 huh\r\n").await?;
        }
        reader.flush().await?;
    }
}

/// Spawn the fixture listener; answers its port.
async fn spawn_smtp(script: Script) -> u16 {
    let (_, acceptor) = tls_pair();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let script = Arc::new(script);
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        if script.starttls {
            let _ = serve_smtp(tcp, script).await;
        } else {
            let tls = acceptor.accept(tcp).await.unwrap();
            let _ = serve_smtp(tls, script).await;
        }
    });
    port
}

const WIRE: &[u8] = b"From: Me <me@x.com>\r\nSubject: hi\r\n\r\n.line starting with a dot\r\nbody";

#[tokio::test]
async fn implicit_tls_submission_runs_the_full_conversation_dot_stuffed() {
    let script = Script::default();
    let body_handle = script.body.clone();
    let port = spawn_smtp(script).await;
    let (client, _) = tls_pair();
    submit_smtp(
        &client,
        &dial_for(port, TransportSecurity::ImplicitTls),
        "me@x.com",
        &["peer@x.com".to_string()],
        WIRE,
    )
    .await
    .expect("submission succeeds");
    let body = body_handle.lock().unwrap().clone();
    let text = String::from_utf8(body).unwrap();
    assert!(
        text.contains("\r\n..line starting with a dot\r\n"),
        "the leading-dot line went out dot-stuffed: {text}"
    );
    assert!(text.contains("Subject: hi"));
}

#[tokio::test]
async fn starttls_submission_upgrades_then_authenticates() {
    let script = Script {
        starttls: true,
        ..Default::default()
    };
    let body_handle = script.body.clone();
    let port = spawn_smtp(script).await;
    let (client, _) = tls_pair();
    submit_smtp(
        &client,
        &dial_for(port, TransportSecurity::StartTls),
        "me@x.com",
        &["peer@x.com".to_string()],
        WIRE,
    )
    .await
    .expect("starttls submission succeeds");
    assert!(!body_handle.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_recipient_refusal_is_permanent() {
    let script = Script {
        rcpt_code: 550,
        ..Default::default()
    };
    let port = spawn_smtp(script).await;
    let (client, _) = tls_pair();
    let err = submit_smtp(
        &client,
        &dial_for(port, TransportSecurity::ImplicitTls),
        "me@x.com",
        &["peer@x.com".to_string()],
        WIRE,
    )
    .await
    .expect_err("the server refuses the recipient");
    assert_eq!(err.kind(), SubmitFailureKind::Permanent);
}

#[tokio::test]
async fn a_mail_from_4xx_is_transient() {
    let script = Script {
        mail_code: 451,
        ..Default::default()
    };
    let port = spawn_smtp(script).await;
    let (client, _) = tls_pair();
    let err = submit_smtp(
        &client,
        &dial_for(port, TransportSecurity::ImplicitTls),
        "me@x.com",
        &["peer@x.com".to_string()],
        WIRE,
    )
    .await
    .expect_err("temporary refusal");
    assert_eq!(err.kind(), SubmitFailureKind::Transient);
}

#[tokio::test]
async fn a_drop_after_data_begins_is_unconfirmed() {
    let script = Script {
        close_after_354: true,
        ..Default::default()
    };
    let port = spawn_smtp(script).await;
    let (client, _) = tls_pair();
    let err = submit_smtp(
        &client,
        &dial_for(port, TransportSecurity::ImplicitTls),
        "me@x.com",
        &["peer@x.com".to_string()],
        WIRE,
    )
    .await
    .expect_err("the server dies mid-DATA");
    assert_eq!(
        err.kind(),
        SubmitFailureKind::Unconfirmed,
        "the message may have been delivered: never auto-retry"
    );
}

#[tokio::test]
async fn an_unreachable_server_is_transient() {
    let (client, _) = tls_pair();
    // Nothing listens on this port (bound and dropped).
    let port = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let err = submit_smtp(
        &client,
        &dial_for(port, TransportSecurity::ImplicitTls),
        "me@x.com",
        &["peer@x.com".to_string()],
        WIRE,
    )
    .await
    .expect_err("nothing to connect to");
    assert_eq!(err.kind(), SubmitFailureKind::Transient);
}

// ── IMAP Sent filing ─────────────────────────────────────────────────────

async fn serve_imap<S>(mut stream: S, recorded: Arc<Mutex<Vec<u8>>>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(&mut stream);
    reader.write_all(b"* OK fixture IMAP\r\n").await?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let upper = line.to_ascii_uppercase();
        if upper.contains(" LOGIN ") {
            reader.write_all(b"a1 OK login\r\n").await?;
        } else if upper.contains(" LIST ") {
            reader
                .write_all(
                    b"* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent Items\"\r\na2 OK list\r\n",
                )
                .await?;
        } else if upper.contains(" APPEND ") {
            assert!(upper.contains("SENT ITEMS"), "the \\Sent folder: {line}");
            reader.write_all(b"+ go\r\n").await?;
            // Read the literal + the trailing CRLF, then the tagged reply read.
            let mut body = Vec::new();
            loop {
                let mut l = String::new();
                if reader.read_line(&mut l).await? == 0 {
                    return Ok(());
                }
                body.extend_from_slice(l.as_bytes());
                if body.ends_with(b"\r\n") {
                    break;
                }
            }
            *recorded.lock().unwrap() = body;
            reader.write_all(b"a3 OK appended\r\n").await?;
        } else if upper.contains(" LOGOUT") {
            reader.write_all(b"a4 OK bye\r\n").await?;
            reader.flush().await?;
            return Ok(());
        } else {
            reader.write_all(b"a9 BAD huh\r\n").await?;
        }
        reader.flush().await?;
    }
}

#[tokio::test]
async fn the_sent_copy_lands_in_the_special_use_folder() {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let (_, acceptor) = tls_pair();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let recorded_server = recorded.clone();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let _ = serve_imap(tls, recorded_server).await;
    });

    let (client, _) = tls_pair();
    let dial = ImapDial {
        addr: format!("127.0.0.1:{port}"),
        server_name: "localhost".to_string(),
        security: TransportSecurity::ImplicitTls,
        username: "u@x.com".to_string(),
        password: "pw".to_string(),
    };
    let filed = b"From: Me <me@x.com>\r\nBcc: hidden@x.com\r\n\r\nbody".to_vec();
    let folder = file_sent_copy(&client, &dial, &filed)
        .await
        .expect("the copy files");
    assert_eq!(folder, "Sent Items");
    let got = recorded.lock().unwrap().clone();
    assert!(
        got.starts_with(b"From: Me <me@x.com>"),
        "the exact filed bytes landed: {}",
        String::from_utf8_lossy(&got)
    );
}
