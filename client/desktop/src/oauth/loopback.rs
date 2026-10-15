// SPDX-License-Identifier: GPL-3.0-only
//! The Google Desktop flow's loopback capture: the host half of the
//! redirect a Google Desktop client type must use, because it supports
//! no custom scheme. The port is baked into the redirect, so the
//! listener binds BEFORE the authorization URL mints: [`begin`] binds
//! the ephemeral port, rebuilds the flow's client over the template
//! config with `http://127.0.0.1:<port>/` as its redirect, and stands a
//! bounded serve thread that captures exactly one callback.
//!
//! The serve shape is the reference's: the browser lands one `GET` whose
//! target carries the authorization response in its query; a request
//! without the callback's shape (a favicon probe, a `POST`) is answered
//! `400` and the listener stands on, because a browser may probe before
//! it redirects. The one callback that parses is answered with the
//! close page, the listener closes (a later connection is refused, the
//! one-shot discipline), and only then does the captured URL cross.
//!
//! The captured URL is the request target made absolute, handed whole:
//! `code`, `state`, and any provider `error` are `astra-oauth`'s to
//! read, exactly as the deep-link capture hands its URL on.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use astra_oauth::{OAuthClient, OAuthError};

/// How long a begun flow's listener stands: the browser may sit on the
/// consent page as long as the person likes, so the bound is minutes,
/// and its passing is surfaced as the capture's failure sentence.
pub(crate) const CAPTURE_BOUND: Duration = Duration::from_mins(5);

/// The loopback redirect without its port: the URI shape a Google
/// Desktop registration carries in Cloud Console, which matches any
/// port (Google's loopback rule). The boot-time Google client carries
/// it as its template redirect; the port a flow binds lands in the
/// redirect that flow's own client mints.
pub(crate) const TEMPLATE_REDIRECT_URI: &str = "http://127.0.0.1/";

/// How often the nonblocking accept loop polls: the listener must wake
/// for its bound, so it never blocks longer than this.
const ACCEPT_POLL: Duration = Duration::from_millis(50);

/// How long an accepted connection may take to hand its request over.
const REQUEST_READ_BOUND: Duration = Duration::from_secs(5);

/// The most request bytes one connection may send before the request is
/// refused: a request line with headers fits far inside this.
const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// The page the captured redirect is answered with: the person's
/// browser shows it as the flow's last word, so it names the app the
/// person returns to. Ported from the reference's host halves.
const CLOSE_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>Astra Mail</title></head><body><p>You can close this tab and return to Astra Mail.</p>\
</body></html>";

/// How the one capture ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoopbackCapture {
    /// The redirect's whole URL, the request target made absolute, read
    /// off the wire and handed on unparsed.
    Received(String),
    /// No redirect arrived: the bound passed or the socket failed, and
    /// the sentence names which.
    Failed(String),
}

/// One begun loopback flow's halves: the client minted over the bound
/// port's redirect, and the receiving end of the one capture.
pub(crate) struct LoopbackFlow {
    /// The flow's client: the template's config with the bound port in
    /// its redirect, so the authorization URL, the exchange, and the
    /// completion's document all name the redirect this flow runs over.
    pub(crate) client: Arc<OAuthClient>,
    /// The one capture: the listener's URL when the redirect lands, or
    /// its failure sentence when the bound passes.
    pub(crate) capture: mpsc::Receiver<LoopbackCapture>,
}

/// Binds the flow's loopback port, mints the flow's client over it, and
/// stands the bounded serve thread: in that order, because the port is
/// the redirect and the URL is minted over the client this returns.
///
/// # Errors
///
/// Answers a sentence when the port cannot be bound, the flow's client
/// cannot be built, or the serve thread cannot start; each leaves
/// nothing bound and no flow begun.
pub(crate) fn begin(template: &OAuthClient, bound: Duration) -> Result<LoopbackFlow, String> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|err| format!("the loopback port could not be bound: {err}"))?;
    // The serve loop polls the listener against its bound, so the socket
    // never blocks in accept: a blocking accept would sleep past the
    // bound and the flow's failure would never surface.
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("the loopback listener could not poll: {err}"))?;
    let address = loopback_address(&listener)?;
    let mut provider = template.provider().clone();
    provider.redirect_uri = format!("http://{address}/");
    let client = OAuthClient::new(provider).map_err(|err: OAuthError| {
        format!("the flow's client could not be built over the loopback redirect: {err}")
    })?;
    let (tx, capture) = mpsc::channel(1);
    std::thread::Builder::new()
        .name("astra-loopback-capture".to_owned())
        .spawn(move || serve(listener, address, bound, &tx))
        .map_err(|err| format!("the capture thread could not be started: {err}"))?;
    Ok(LoopbackFlow {
        client: Arc::new(client),
        capture,
    })
}

/// The listener's own loopback address, proving the bind landed where
/// the redirect claims.
fn loopback_address(listener: &TcpListener) -> Result<SocketAddrV4, String> {
    match listener.local_addr() {
        Ok(SocketAddr::V4(address)) if address.ip().is_loopback() => Ok(address),
        _ => Err("the listener did not bind to the IPv4 loopback".to_owned()),
    }
}

/// Serves the one capture: polls the nonblocking listener until a
/// callback-shaped request lands, the bound passes, or the socket
/// fails, then hands the outcome to the flow's waiter and ends.
fn serve(
    listener: TcpListener,
    address: SocketAddrV4,
    bound: Duration,
    tx: &mpsc::Sender<LoopbackCapture>,
) {
    let deadline = Instant::now() + bound;
    let outcome = loop {
        if Instant::now() >= deadline {
            break LoopbackCapture::Failed(
                "the bound passed before any redirect arrived".to_owned(),
            );
        }
        match listener.accept() {
            Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                let Some(target) = read_callback_request(&mut stream) else {
                    let _ = write_bad_request(&mut stream);
                    continue;
                };
                // The one-shot discipline: the browser gets its answer,
                // the listener closes before the capture crosses, and a
                // later connection to this port is refused rather than
                // answered by a flow that is already finished.
                let _ = write_close_page(&mut stream);
                drop(stream);
                drop(listener);
                break LoopbackCapture::Received(format!("http://{address}{target}"));
            }
            // A connection from anywhere but the loopback never carries
            // this flow's redirect.
            Ok((stream, _)) => drop(stream),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL);
            }
            Err(err) => {
                break LoopbackCapture::Failed(format!("the listener socket failed: {err}"));
            }
        }
    };
    // The waiter may be gone (a cancelled flow drops its receiver); the
    // capture still ends the thread and the browser still got its page.
    let _ = tx.blocking_send(outcome);
}

/// Reads one request off an accepted connection and hands its target
/// back only when it is callback-shaped: a `GET` whose query carries a
/// `state` and a `code` or `error`. Anything else is `None`, so the
/// caller answers `400` and stands on.
fn read_callback_request(stream: &mut TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(REQUEST_READ_BOUND)).ok()?;
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    while request.len() < MAX_REQUEST_BYTES {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            break;
        }
    }
    if request.len() >= MAX_REQUEST_BYTES {
        return None;
    }
    parse_request_target(&request).map(str::to_owned)
}

/// The callback-shaped request target of a raw request, or `None`.
fn parse_request_target(request: &[u8]) -> Option<&str> {
    let request = std::str::from_utf8(request).ok()?;
    let mut parts = request.lines().next()?.split_ascii_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    if !target.starts_with('/')
        || parts
            .next()
            .is_none_or(|version| !version.starts_with("HTTP/"))
    {
        return None;
    }
    let query = target.split_once('?')?.1;
    let has_state = query.split('&').any(|part| part.starts_with("state="));
    let has_result = query
        .split('&')
        .any(|part| part.starts_with("code=") || part.starts_with("error="));
    (has_state && has_result).then_some(target)
}

/// Answers the one captured redirect with the close page.
fn write_close_page(stream: &mut TcpStream) -> std::io::Result<()> {
    write_response(stream, "200 OK", "text/html; charset=utf-8", CLOSE_PAGE)
}

/// Answers a request that never carried a callback.
fn write_bad_request(stream: &mut TcpStream) -> std::io::Result<()> {
    write_response(
        stream,
        "400 Bad Request",
        "text/plain; charset=utf-8",
        "Bad request",
    )
}

/// Writes one whole HTTP response and closes the connection.
fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )?;
    stream.flush()
}

#[cfg(test)]
#[path = "loopback_tests.rs"]
mod tests;
