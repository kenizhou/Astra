// SPDX-License-Identifier: GPL-3.0-only
//! The bypass transport (SP4b spec D3): deliberately small and explicit
//! about what it is. The SMTP submitter runs EHLO, `AUTH PLAIN`,
//! `MAIL FROM`, `RCPT TO`, dot-stuffed `DATA`, `RSET` on error, over the
//! account's dialled settings and the host-built TLS connector; the Sent
//! filing is a minimal IMAP `LOGIN` + special-use `LIST` + `APPEND`. The
//! recorded known gap stands: the engine's own submitter hardened partial
//! DATA, AUTH renewal, and deadline discipline over releases; this subset
//! is honest about being smaller, and every failure is typed, never silent.

use std::time::Duration;

use base64::Engine;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

/// Per-step stall bound: a server that stops answering must not hang the
/// drain forever.
const STEP_TIMEOUT: Duration = Duration::from_secs(15);

/// How the connection is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportSecurity {
    /// TLS from the first byte (ports 465/993).
    ImplicitTls,
    /// Cleartext upgraded in place (ports 587/143); credentials never cross
    /// before the upgrade.
    StartTls,
}

/// An SMTP submission endpoint with credentials.
#[derive(Debug, Clone)]
pub struct SmtpDial {
    /// The dial address, `host:port`.
    pub addr: String,
    /// The TLS server name for SNI/verification.
    pub server_name: String,
    /// How the connection is secured.
    pub security: TransportSecurity,
    /// The login username.
    pub username: String,
    /// The login password.
    pub password: String,
}

/// An IMAP endpoint for the Sent-copy filing.
#[derive(Debug, Clone)]
pub struct ImapDial {
    /// The dial address, `host:port`.
    pub addr: String,
    /// The TLS server name for SNI/verification.
    pub server_name: String,
    /// How the connection is secured.
    pub security: TransportSecurity,
    /// The login username.
    pub username: String,
    /// The login password.
    pub password: String,
}

/// How a submit attempt failed. The class drives the outbox state, not a
/// retry policy baked in here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitFailureKind {
    /// Nothing was accepted by the server: a retry is safe (connection or
    /// protocol failure before `DATA`, or a 4xx).
    Transient,
    /// The server refused the message (5xx): no automatic retry.
    Permanent,
    /// The connection died after `DATA` began: the message may or may not
    /// have been delivered. Never auto-retried; the user's answer settles it
    /// (the engine's needs-confirmation shape).
    Unconfirmed,
}

/// A typed submit failure. `Display` is the sentence the outbox row keeps.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct SubmitFailure {
    kind: SubmitFailureKind,
    message: String,
}

impl SubmitFailure {
    fn new(kind: SubmitFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The failure's class.
    pub fn kind(&self) -> SubmitFailureKind {
        self.kind
    }
}

/// Submit `wire_bytes` to `recipients` from `from` through `dial`. The
/// conversation is the minimal honest subset: greeting, EHLO, (STARTTLS),
/// AUTH PLAIN, MAIL FROM, RCPT TO per recipient, DATA dot-stuffed, RSET on
/// a mid-conversation refusal, QUIT.
///
/// # Errors
/// [`SubmitFailure`] typed by where the conversation broke: `Transient`
/// before `DATA` (a retry is safe), `Permanent` on a 5xx refusal,
/// `Unconfirmed` when the connection died after `DATA` began.
pub async fn submit_smtp(
    connector: &TlsConnector,
    dial: &SmtpDial,
    from: &str,
    recipients: &[String],
    wire_bytes: &[u8],
) -> Result<(), SubmitFailure> {
    let tcp = TcpStream::connect(&dial.addr).await.map_err(|e| {
        SubmitFailure::new(
            SubmitFailureKind::Transient,
            format!("smtp connect {}: {e}", dial.addr),
        )
    })?;
    tcp.set_nodelay(true).ok();
    match dial.security {
        TransportSecurity::ImplicitTls => {
            let tls = upgrade(connector, tcp, &dial.server_name).await?;
            run_smtp(
                BufReader::new(tls),
                false,
                &dial.server_name,
                &dial.username,
                &dial.password,
                from,
                recipients,
                wire_bytes,
            )
            .await
        }
        TransportSecurity::StartTls => {
            let mut plain = LineStream::new(BufReader::new(tcp));
            plain.read_reply().await?;
            plain.command(&format!("EHLO {}", dial.server_name)).await?;
            plain.command("STARTTLS").await?;
            // A conformant server sends nothing between the 220 and the
            // handshake; refuse to carry buffered plaintext across the TLS
            // boundary (CVE-2011-0411 class).
            let stream = plain.into_inner()?;
            let tls = upgrade(connector, stream, &dial.server_name).await?;
            run_smtp(
                BufReader::new(tls),
                true,
                &dial.server_name,
                &dial.username,
                &dial.password,
                from,
                recipients,
                wire_bytes,
            )
            .await
        }
    }
}

/// File `filed_bytes` as the sender's Sent copy: LOGIN, special-use LIST
/// for `\Sent` (falling back to `Sent`), APPEND with a synchronising
/// literal, LOGOUT. Returns the folder the copy landed in.
///
/// # Errors
/// A plain `String` error: the filing is best-effort beside the delivery,
/// so the caller logs and continues rather than typing the failure.
pub async fn file_sent_copy(
    connector: &TlsConnector,
    dial: &ImapDial,
    filed_bytes: &[u8],
) -> Result<String, String> {
    let tcp = TcpStream::connect(&dial.addr)
        .await
        .map_err(|e| format!("imap connect {}: {e}", dial.addr))?;
    tcp.set_nodelay(true).ok();
    match dial.security {
        TransportSecurity::ImplicitTls => {
            let tls = upgrade_string(connector, tcp, &dial.server_name).await?;
            run_imap_append(BufReader::new(tls), dial, filed_bytes).await
        }
        TransportSecurity::StartTls => {
            let mut stream = ImapStream::new(BufReader::new(tcp));
            stream.read_untagged().await?;
            stream
                .command("a1", "STARTTLS")
                .await
                .map_err(|e| format!("imap starttls: {e}"))?;
            let inner = stream.into_inner_string()?;
            let tls = upgrade_string(connector, inner, &dial.server_name).await?;
            run_imap_append(BufReader::new(tls), dial, filed_bytes).await
        }
    }
}

async fn upgrade<S>(
    connector: &TlsConnector,
    tcp: S,
    server_name: &str,
) -> Result<tokio_rustls::client::TlsStream<S>, SubmitFailure>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let name = ServerName::try_from(server_name.to_owned()).map_err(|e| {
        SubmitFailure::new(
            SubmitFailureKind::Permanent,
            format!("tls server name {server_name}: {e}"),
        )
    })?;
    tokio::time::timeout(STEP_TIMEOUT, connector.connect(name, tcp))
        .await
        .map_err(|_| {
            SubmitFailure::new(
                SubmitFailureKind::Transient,
                format!("tls handshake with {server_name} timed out"),
            )
        })?
        .map_err(|e| {
            SubmitFailure::new(
                SubmitFailureKind::Transient,
                format!("tls handshake with {server_name}: {e}"),
            )
        })
}

async fn upgrade_string<S>(
    connector: &TlsConnector,
    tcp: S,
    server_name: &str,
) -> Result<tokio_rustls::client::TlsStream<S>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    upgrade(connector, tcp, server_name)
        .await
        .map_err(|e| e.to_string())
}

/// A line-based SMTP stream with multiline-reply assembly.
struct LineStream<S> {
    inner: BufReader<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> LineStream<S> {
    fn new(inner: BufReader<S>) -> Self {
        Self { inner }
    }

    /// Read a (possibly multiline) reply: the code and the joined text.
    async fn read_reply(&mut self) -> Result<(u16, String), SubmitFailure> {
        let mut lines = Vec::new();
        for _ in 0..64 {
            let mut line = String::new();
            let n = tokio::time::timeout(STEP_TIMEOUT, self.inner.read_line(&mut line))
                .await
                .map_err(|_| {
                    SubmitFailure::new(
                        SubmitFailureKind::Transient,
                        "smtp: the server sent nothing in time",
                    )
                })?
                .map_err(|e| {
                    SubmitFailure::new(SubmitFailureKind::Transient, format!("smtp read: {e}"))
                })?;
            if n == 0 {
                return Err(SubmitFailure::new(
                    SubmitFailureKind::Transient,
                    "smtp: connection closed",
                ));
            }
            let trimmed = line.trim_end();
            let code: u16 = trimmed
                .get(0..3)
                .and_then(|c| c.parse().ok())
                .ok_or_else(|| {
                    SubmitFailure::new(
                        SubmitFailureKind::Permanent,
                        format!("smtp: malformed reply: {trimmed}"),
                    )
                })?;
            lines.push(trimmed.get(4..).unwrap_or("").to_string());
            if trimmed.as_bytes().get(3) != Some(&b'-') {
                return Ok((code, lines.join(" ")));
            }
        }
        Err(SubmitFailure::new(
            SubmitFailureKind::Permanent,
            "smtp: multiline reply exceeded the line cap",
        ))
    }

    async fn write_line(&mut self, line: &str) -> Result<(), SubmitFailure> {
        tokio::time::timeout(
            STEP_TIMEOUT,
            self.inner.write_all(format!("{line}\r\n").as_bytes()),
        )
        .await
        .map_err(|_| SubmitFailure::new(SubmitFailureKind::Transient, "smtp: write timed out"))?
        .map_err(|e| {
            SubmitFailure::new(SubmitFailureKind::Transient, format!("smtp write: {e}"))
        })?;
        tokio::time::timeout(STEP_TIMEOUT, self.inner.flush())
            .await
            .map_err(|_| SubmitFailure::new(SubmitFailureKind::Transient, "smtp: flush timed out"))?
            .map_err(|e| {
                SubmitFailure::new(SubmitFailureKind::Transient, format!("smtp flush: {e}"))
            })
    }

    /// One command expecting a 2xx/3xx answer; 4xx is Transient, 5xx
    /// Permanent.
    async fn command(&mut self, line: &str) -> Result<String, SubmitFailure> {
        self.write_line(line).await?;
        self.expect_ok(line).await
    }

    async fn expect_ok(&mut self, what: &str) -> Result<String, SubmitFailure> {
        let (code, text) = self.read_reply().await?;
        match code / 100 {
            2 | 3 => Ok(text),
            4 => Err(SubmitFailure::new(
                SubmitFailureKind::Transient,
                format!("smtp: {what} refused ({code} {text})"),
            )),
            _ => Err(SubmitFailure::new(
                SubmitFailureKind::Permanent,
                format!("smtp: {what} refused ({code} {text})"),
            )),
        }
    }

    fn into_inner(self) -> Result<S, SubmitFailure> {
        if !self.inner.buffer().is_empty() {
            return Err(SubmitFailure::new(
                SubmitFailureKind::Permanent,
                "smtp: unexpected buffered data after STARTTLS (possible command injection)",
            ));
        }
        Ok(self.inner.into_inner())
    }
}

/// Dot-stuffs a CRLF-delimited message: any line beginning with `.` gets a
/// second leading `.` so it is not mistaken for the terminator (RFC 5321
/// §4.5.2).
fn dot_stuff(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(message.len() + 16);
    let mut start = 0;
    while start < message.len() {
        let end = message[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(message.len(), |p| start + p + 1);
        let line = &message[start..end];
        if line.first() == Some(&b'.') {
            out.push(b'.');
        }
        out.extend_from_slice(line);
        start = end;
    }
    out
}

#[allow(clippy::too_many_arguments)] // the conversation surface; bundling adds ceremony without clarity
async fn run_smtp<S>(
    stream: BufReader<S>,
    greeting_read: bool,
    server_name: &str,
    username: &str,
    password: &str,
    from: &str,
    recipients: &[String],
    wire_bytes: &[u8],
) -> Result<(), SubmitFailure>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut smtp = LineStream::new(stream);
    // Greeting (implicit TLS: the server speaks first; the STARTTLS path
    // already read it before the upgrade and the server sends none after).
    if !greeting_read {
        smtp.read_reply().await?;
    }
    smtp.command(&format!("EHLO {server_name}")).await?;
    let auth =
        base64::engine::general_purpose::STANDARD.encode(format!("\0{username}\0{password}"));
    smtp.command(&format!("AUTH PLAIN {auth}")).await?;
    smtp.command(&format!("MAIL FROM:<{from}>")).await?;
    for recipient in recipients {
        smtp.command(&format!("RCPT TO:<{recipient}>")).await?;
    }
    smtp.command("DATA").await?;
    // From here on, a break is unconfirmed: the server may have taken the
    // message. Best-effort RSET on a refusal; on an IO break there is no
    // conversation left to reset.
    let body_result: Result<(), SubmitFailure> = async {
        let stuffed = dot_stuff(wire_bytes);
        tokio::time::timeout(STEP_TIMEOUT, smtp.inner.write_all(&stuffed))
            .await
            .map_err(|_| {
                SubmitFailure::new(SubmitFailureKind::Unconfirmed, "smtp: DATA write timed out")
            })?
            .map_err(|e| {
                SubmitFailure::new(
                    SubmitFailureKind::Unconfirmed,
                    format!("smtp: DATA write: {e}"),
                )
            })?;
        tokio::time::timeout(STEP_TIMEOUT, smtp.inner.write_all(b"\r\n.\r\n"))
            .await
            .map_err(|_| {
                SubmitFailure::new(
                    SubmitFailureKind::Unconfirmed,
                    "smtp: DATA terminator timed out",
                )
            })?
            .map_err(|e| {
                SubmitFailure::new(
                    SubmitFailureKind::Unconfirmed,
                    format!("smtp: DATA terminator: {e}"),
                )
            })?;
        tokio::time::timeout(STEP_TIMEOUT, smtp.inner.flush())
            .await
            .map_err(|_| {
                SubmitFailure::new(SubmitFailureKind::Unconfirmed, "smtp: flush timed out")
            })?
            .map_err(|e| {
                SubmitFailure::new(SubmitFailureKind::Unconfirmed, format!("smtp: flush: {e}"))
            })?;
        let (code, text) = smtp.read_reply().await.map_err(|e| {
            SubmitFailure::new(
                SubmitFailureKind::Unconfirmed,
                format!("smtp: no answer after DATA: {e}"),
            )
        })?;
        match code / 100 {
            2 => Ok(()),
            4 => Err(SubmitFailure::new(
                SubmitFailureKind::Transient,
                format!("smtp: DATA refused ({code} {text})"),
            )),
            _ => Err(SubmitFailure::new(
                SubmitFailureKind::Permanent,
                format!("smtp: DATA refused ({code} {text})"),
            )),
        }
    }
    .await;
    match &body_result {
        Ok(()) => {
            let _ = smtp.write_line("QUIT").await;
        }
        Err(f) if f.kind() != SubmitFailureKind::Unconfirmed => {
            let _ = smtp.write_line("RSET").await;
        }
        Err(_) => {}
    }
    body_result
}

/// A line-based IMAP stream (tagged commands + untagged replies), just big
/// enough for the filing.
struct ImapStream<S> {
    inner: BufReader<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> ImapStream<S> {
    fn new(inner: BufReader<S>) -> Self {
        Self { inner }
    }

    async fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        let n = tokio::time::timeout(STEP_TIMEOUT, self.inner.read_line(&mut line))
            .await
            .map_err(|_| "imap: the server sent nothing in time".to_string())?
            .map_err(|e| format!("imap read: {e}"))?;
        if n == 0 {
            return Err("imap: connection closed".to_string());
        }
        Ok(line.trim_end().to_string())
    }

    async fn read_untagged(&mut self) -> Result<String, String> {
        self.read_line().await
    }

    async fn write_raw(&mut self, bytes: &[u8]) -> Result<(), String> {
        tokio::time::timeout(STEP_TIMEOUT, self.inner.write_all(bytes))
            .await
            .map_err(|_| "imap: write timed out".to_string())?
            .map_err(|e| format!("imap write: {e}"))?;
        tokio::time::timeout(STEP_TIMEOUT, self.inner.flush())
            .await
            .map_err(|_| "imap: flush timed out".to_string())?
            .map_err(|e| format!("imap flush: {e}"))
    }

    /// One tagged command; answers the untagged lines. Err on NO/BAD.
    async fn command(&mut self, tag: &str, line: &str) -> Result<Vec<String>, String> {
        self.write_raw(format!("{tag} {line}\r\n").as_bytes())
            .await?;
        let mut untagged = Vec::new();
        loop {
            let reply = self.read_line().await?;
            if let Some(rest) = reply.strip_prefix(&format!("{tag} ")) {
                if rest.starts_with("OK") {
                    return Ok(untagged);
                }
                return Err(format!("imap: {line} refused: {rest}"));
            }
            untagged.push(reply);
        }
    }

    fn into_inner_string(self) -> Result<S, String> {
        if !self.inner.buffer().is_empty() {
            return Err("imap: unexpected buffered data after STARTTLS".to_string());
        }
        Ok(self.inner.into_inner())
    }
}

/// IMAP-quote a string atom (LOGIN credentials, folder names).
fn imap_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Extract the last quoted token from a LIST reply line (the folder name).
fn list_folder_name(line: &str) -> Option<String> {
    let mut names = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let end = after.find('"')?;
        names.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    names.into_iter().last()
}

async fn run_imap_append<S>(
    stream: BufReader<S>,
    dial: &ImapDial,
    filed_bytes: &[u8],
) -> Result<String, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut imap = ImapStream::new(stream);
    imap.read_untagged().await?;
    imap.command(
        "a1",
        &format!(
            "LOGIN {} {}",
            imap_quote(&dial.username),
            imap_quote(&dial.password)
        ),
    )
    .await
    .map_err(|e| format!("imap login: {e}"))?;
    // The Sent folder by special-use attribute; fall back to the common name.
    let listing = imap
        .command("a2", "LIST (SPECIAL-USE) \"\" \"*\"")
        .await
        .unwrap_or_default();
    let folder = listing
        .iter()
        .find(|line| line.to_ascii_lowercase().contains("\\sent"))
        .and_then(|line| list_folder_name(line))
        .unwrap_or_else(|| "Sent".to_string());
    // Synchronising literal: the server must bless the size before the bytes.
    imap.write_raw(
        format!(
            "a3 APPEND {} {{{}}}\r\n",
            imap_quote(&folder),
            filed_bytes.len()
        )
        .as_bytes(),
    )
    .await?;
    let go = imap.read_line().await?;
    if !go.starts_with('+') {
        return Err(format!("imap append refused the literal: {go}"));
    }
    imap.write_raw(filed_bytes).await?;
    imap.write_raw(b"\r\n").await?;
    loop {
        let reply = imap.read_line().await?;
        if let Some(rest) = reply.strip_prefix("a3 ") {
            if rest.starts_with("OK") {
                break;
            }
            return Err(format!("imap append refused: {rest}"));
        }
    }
    let _ = imap.command("a4", "LOGOUT").await;
    Ok(folder)
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;
