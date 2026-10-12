// SPDX-License-Identifier: GPL-3.0-only
//! The transport: where the server listens, and who may connect.
//!
//! # The endpoint
//!
//! The endpoint is derived once, by the layer that owns the app's data
//! directory (the desktop host), and crosses to the relay as its argument;
//! the relay never computes it. On Windows the pipe name is the data
//! directory's final component plus `.mcp`, so the standard directory
//! yields `\\.\pipe\com.kenizhou.astra.mcp` and a dev data directory
//! beside it yields a different pipe: two builds on one machine never
//! steal each other's clients. On Unix the socket is `mcp.sock` inside the
//! data directory. The name is per app, not per OS user: a second user's
//! instance of the same build finds the name taken and fails loudly, which
//! is honest and visible, never a silent takeover.
//!
//! # The peer check, and what is honestly provable here
//!
//! On Windows the boundary is the pipe's own security descriptor, not a
//! comparison this crate runs: an instance created with null security
//! attributes inherits the creating token's default DACL (the user,
//! SYSTEM, and Administrators), `reject_remote_clients` has the operating
//! system refuse the SMB path in from another machine, and
//! `first_pipe_instance` turns a name collision into a loud bind failure
//! rather than a hijack. There is no token or SID comparison code because
//! reading the connecting process's token has no safe API on the pinned
//! tokio (the raw security-attribute builder is `unsafe`, and this
//! workspace forbids `unsafe` outright). What a test can prove on one
//! machine is the positive half: a same-user client connects and is
//! served. The negative halves (another user refused, a remote client
//! refused) are enforced by the operating system against that descriptor,
//! and reproducing either needs a second account or a second machine,
//! which this harness has neither of; that shape is recorded here and in
//! the design's known gaps rather than claimed as covered.
//!
//! On Unix the filesystem carries the boundary: the socket's parent
//! directory is 0700, the socket itself is 0600, and every accepted
//! connection's peer uid is checked against the socket file's owner.
//!
//! # One code path
//!
//! [`Endpoint`] and [`Listener`] are one facade over the two platform
//! halves, which own only the OS accept dance; every accepted connection
//! runs the server module's own framing loop, unchanged, so there is no
//! second framing implementation anywhere.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::server::Server;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// The `sun_path` limit for a Unix domain socket address, in bytes,
/// **including** the trailing NUL: 104 on macOS and the BSDs, 108 on
/// Linux. The smaller bounds every host so a path that validates on one
/// validates on all.
pub const MAX_SOCKET_PATH: usize = 104;

/// Why an endpoint string cannot be listened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    /// The endpoint string was empty.
    Empty,
    /// A Windows pipe name that is not under `\\.\pipe\`.
    NotAPipeName,
    /// A Unix socket path that does not fit `sun_path`, with the length
    /// that failed so the message names the cause instead of a bind error
    /// nobody connects to a path limit.
    TooLong {
        /// How many bytes the path needed.
        length: usize,
    },
}

impl core::fmt::Display for EndpointError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => write!(f, "the MCP endpoint is empty"),
            Self::NotAPipeName => write!(f, "a Windows MCP endpoint must be a \\\\.\\pipe\\ name"),
            Self::TooLong { length } => write!(
                f,
                "the MCP socket path is {length} bytes; a Unix socket allows at most {} \
                 including the terminator",
                MAX_SOCKET_PATH - 1,
            ),
        }
    }
}

impl std::error::Error for EndpointError {}

/// A validated place to listen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// A Windows named pipe, `\\.\pipe\...`.
    Pipe(String),
    /// A Unix domain socket at this filesystem path.
    Unix(String),
}

impl Endpoint {
    /// Validates a host-supplied endpoint string.
    ///
    /// A value starting `\\.\pipe\` is a named pipe; anything else is a
    /// filesystem path. The discrimination is on the value rather than on
    /// the host, so the rule is testable anywhere a test runs.
    ///
    /// # Errors
    ///
    /// See [`EndpointError`].
    pub fn parse(raw: &str) -> Result<Self, EndpointError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(EndpointError::Empty);
        }
        if raw.starts_with(r"\\.\pipe\") {
            if raw.len() <= r"\\.\pipe\".len() {
                return Err(EndpointError::NotAPipeName);
            }
            return Ok(Self::Pipe(raw.to_owned()));
        }
        // `sun_path` counts bytes, not characters, and holds a trailing NUL.
        let length = raw.len() + 1;
        if length > MAX_SOCKET_PATH {
            return Err(EndpointError::TooLong { length });
        }
        Ok(Self::Unix(raw.to_owned()))
    }

    /// The endpoint as the string the host hands the relay as its
    /// argument: one derivation, in the layer that owns the data
    /// directory, round-tripped verbatim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Unix(path) | Self::Pipe(path) => path,
        }
    }

    /// The endpoint for the app's data directory; see the module docs for
    /// the derivation and its collision rules.
    #[must_use]
    pub fn for_data_dir(data_dir: &std::path::Path) -> Self {
        if cfg!(windows) {
            let leaf = data_dir
                .file_name()
                .map(|leaf| leaf.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Self::Pipe(format!(r"\\.\pipe\{leaf}.mcp"));
        }
        Self::Unix(data_dir.join("mcp.sock").to_string_lossy().into_owned())
    }
}

/// The platform listener type this build binds.
#[cfg(windows)]
type Platform = windows::PipeListener;
#[cfg(unix)]
type Platform = unix::SocketListener;

/// A bound MCP listener: one platform endpoint with its accept loop ready.
///
/// Dropping the accept future (aborting the task that runs it) closes the
/// endpoint: on Unix the socket file is unlinked, so a client dialling a
/// stopped listener is refused rather than left hanging.
pub struct Listener {
    /// The bound platform endpoint.
    platform: Platform,
}

impl core::fmt::Debug for Listener {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Listener").finish_non_exhaustive()
    }
}

impl Listener {
    /// Binds `endpoint`.
    ///
    /// # Errors
    ///
    /// Whatever the platform bind reports, chiefly a name another
    /// listener owns (a loud failure by design, never a takeover) or a
    /// path that cannot be made this user's alone.
    pub fn bind(endpoint: &Endpoint) -> std::io::Result<Self> {
        let platform = match endpoint {
            #[cfg(windows)]
            Endpoint::Pipe(name) => windows::bind(name)?,
            #[cfg(unix)]
            Endpoint::Unix(path) => unix::bind(path)?,
            // A pipe name on a Unix host, or a socket path on Windows: a
            // host bug, not a user one.
            #[allow(unreachable_patterns)]
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "this platform hosts no such endpoint",
                ));
            }
        };
        Ok(Self { platform })
    }

    /// Accepts connections until the future is dropped, serving each on
    /// `server` with the server module's own connection loop, unchanged.
    pub async fn accept(self, server: Arc<Server>) {
        self.platform.accept(server).await;
    }
}

/// Serves one accepted connection: both platform halves hand every stream
/// to the same framing loop, so the transport owns none of the protocol.
fn spawn_connection<S>(server: Arc<Server>, stream: S)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        server.serve_connection(stream).await;
    });
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;

#[cfg(all(test, windows))]
#[path = "transport_pipe_tests.rs"]
mod pipe_tests;
