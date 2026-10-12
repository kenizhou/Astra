// SPDX-License-Identifier: GPL-3.0-only
//! `astra-mcp-relay`: the stdio relay an MCP client spawns to reach a
//! running Astra Mail.
//!
//! # Why a relay at all
//!
//! An MCP client spawns its server as a child process and talks to it
//! over stdio. But the **running app** owns the mail store, the live
//! connections, and the credentials. A standalone server process would
//! mean a second writer on one store and a second copy of the user's
//! secrets. The relay resolves that: the client gets the stdio it
//! expects, the app stays the single owner, and what crosses between them
//! is bytes.
//!
//! # A byte pipe, and nothing else
//!
//! This relay has **no protocol knowledge at all**: not the frame, not
//! the version, not the id. It copies stdin to the endpoint and the
//! endpoint to stdout, verbatim, so N bytes in produce exactly N bytes
//! out and a hostile byte is a byte. The framing, the handshake, and the
//! tool surface all live in the app's listener, one implementation of
//! each. (mail-calendar's shim reads the frame to answer an unreachable
//! app with a JSON-RPC error; that needs the request's id, which is
//! exactly the knowledge this design refuses, so the deviation is
//! deliberate and recorded in the design's known gaps: the relay exits
//! non-zero with the diagnostic on stderr, and the client reports a
//! failed server, which the user resolves by opening the app.)
//!
//! # The contracts that must never break
//!
//! Nothing but relayed bytes ever reaches stdout; every diagnostic goes
//! to stderr. One stray line on stdout desynchronizes the client's parser
//! and the server looks broken. And the endpoint is **passed in**, never
//! derived: the host owns the one derivation (from the app's data
//! directory), and the client's config carries the exact value it
//! generated, so the relay and the app can never disagree about where to
//! meet.
//!
//! # One code path, two transports
//!
//! The endpoint is a named pipe on Windows and a Unix domain socket
//! elsewhere, both over tokio: overlapped I/O is the only way a Windows
//! pipe carries a read and a write at the same time without hand-rolled
//! `unsafe`, which this workspace forbids, and tokio's socket client
//! keeps the same shape for the pump, so both transports run the one
//! [`relay`] loop below.

use std::io;
use std::process::ExitCode;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

/// `ERROR_PIPE_BUSY`: every instance of the pipe is connected and the
/// server has not finished creating the next one. A self-clearing window,
/// worth riding out rather than reporting the app as unreachable.
#[cfg(windows)]
const ERROR_PIPE_BUSY: i32 = 231;

/// How many times, and how long apart, to retry a busy pipe. Bounded: the
/// point is to ride out an instance being replaced, never to sit waiting
/// for an app that is not running. Windows only: the busy window is a
/// named-pipe behaviour, and the Unix dial has no such state.
#[cfg(windows)]
const BUSY_RETRIES: u32 = 10;
#[cfg(windows)]
const BUSY_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

fn main() -> ExitCode {
    let Some(endpoint) = endpoint_argument() else {
        eprintln!(
            "astra-mcp-relay: usage: astra-mcp-relay --endpoint <pipe name or socket path>\n\
             Astra Mail's advanced settings generate the exact value for the client's config."
        );
        return ExitCode::FAILURE;
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            // stderr, never stdout: a diagnostic on stdout desynchronizes
            // the client's parser.
            eprintln!("astra-mcp-relay: could not start the I/O runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(relay(&endpoint)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("astra-mcp-relay: could not reach Astra Mail: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Reads `--endpoint <value>` (or `--endpoint=<value>`) from the command
/// line. The value is the endpoint itself, used verbatim.
fn endpoint_argument() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--endpoint=") {
            return Some(value.to_owned());
        }
        if arg == "--endpoint" {
            return args.next();
        }
    }
    None
}

/// Connects to the endpoint and pumps bytes both ways until stdin closes.
async fn relay(endpoint: &str) -> io::Result<()> {
    let stream = connect(endpoint).await?;
    pump(stream).await
}

/// Opens the endpoint, riding out the busy window a replacing pipe
/// instance leaves on Windows.
#[cfg(windows)]
async fn connect(endpoint: &str) -> io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    let mut attempt = 0;
    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint) {
            Ok(client) => return Ok(client),
            Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY) && attempt < BUSY_RETRIES => {
                attempt += 1;
                tokio::time::sleep(BUSY_RETRY_DELAY).await;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Opens the endpoint's socket.
#[cfg(unix)]
async fn connect(endpoint: &str) -> io::Result<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(endpoint).await
}

/// Copies stdin to the endpoint and the endpoint to stdout, both verbatim
/// and concurrently, until stdin closes: that is the client being done
/// with the relay, so what it already answered is flushed and the process
/// ends.
///
/// The two directions are independent tasks over one stream, which is the
/// whole point of the overlapped I/O: a reply reaches stdout while the
/// next request is still being written, and the app can speak first.
async fn pump<S>(stream: S) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut from_app, mut to_app) = tokio::io::split(stream);
    let replies = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        let _ = tokio::io::copy(&mut from_app, &mut stdout).await;
        let _ = stdout.flush().await;
    });
    let mut stdin = tokio::io::stdin();
    tokio::io::copy(&mut stdin, &mut to_app).await?;
    let _ = to_app.flush().await;
    // stdin closed: the client is done with us. What the app already
    // answered goes out before the process ends.
    let mut stdout = tokio::io::stdout();
    let _ = stdout.flush().await;
    replies.abort();
    Ok(())
}
