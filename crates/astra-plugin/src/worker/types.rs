// SPDX-License-Identifier: GPL-3.0-only
//! The worker host's public types: the supervision state, the error surface,
//! the events a worker emits outside a call answer, and the spawn
//! configuration. Kept apart from the host itself so the host's module stays
//! under the repository's 500-line cap.

use std::{error::Error, fmt, future::Future, path::PathBuf, pin::Pin, sync::Arc, time::Duration};

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncWrite};

use crate::{Permission, RouteError, AOM_VERSION, PLUGIN_API_LEVEL};

/// The supervision state of one worker. `Lost` is terminal for the instance;
/// the host discovers a fresh one instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerState {
    /// Not spawned, or shut down deliberately.
    Down,
    /// Spawned, handshake not yet complete.
    Starting,
    /// Handshake complete; calls may flow.
    Ready,
    /// Died and awaiting or running restart attempt `attempt`.
    Restarting {
        /// The 1-based restart attempt in flight.
        attempt: u32,
    },
    /// The restart budget is exhausted, or the handshake failed fatally.
    Lost,
}

/// Why a worker operation failed.
#[derive(Debug)]
pub enum WorkerError {
    /// The process failed to spawn.
    Spawn(String),
    /// A pipe write or read failed.
    Io(String),
    /// The ready handshake did not complete inside the init timeout.
    InitTimeout,
    /// The worker's ready named versions this host does not speak. Fatal:
    /// never retried.
    VersionMismatch {
        /// The API level the worker declared.
        found_api_level: u32,
        /// The AOM version the worker declared.
        found_aom_version: u32,
    },
    /// The worker's ready named methods its manifest never declared. Fatal:
    /// never retried.
    UndeclaredMethods(Vec<String>),
    /// A line arrived that was not a worker message, or a ready arrived after
    /// the handshake.
    Protocol(String),
    /// The worker is not Ready (down, starting, restarting, lost, or shut
    /// down).
    NotReady,
    /// The worker died with the call in flight.
    Died,
    /// The router refused the call before it reached the pipe.
    Refused(RouteError),
    /// The worker answered the call with an error.
    Worker(String),
}

impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerError::Spawn(error) => write!(f, "the worker failed to spawn: {error}"),
            WorkerError::Io(error) => write!(f, "worker pipe error: {error}"),
            WorkerError::InitTimeout => write!(f, "the worker's ready handshake timed out"),
            WorkerError::VersionMismatch {
                found_api_level,
                found_aom_version,
            } => write!(
                f,
                "the worker speaks API level {found_api_level} / AOM {found_aom_version}, this host speaks {PLUGIN_API_LEVEL} / {AOM_VERSION}"
            ),
            WorkerError::UndeclaredMethods(methods) => {
                write!(
                    f,
                    "the worker serves methods its manifest does not declare: {methods:?}"
                )
            }
            WorkerError::Protocol(error) => write!(f, "worker protocol violation: {error}"),
            WorkerError::NotReady => write!(f, "the worker is not ready"),
            WorkerError::Died => write!(f, "the worker died with the call in flight"),
            WorkerError::Refused(error) => write!(f, "{error}"),
            WorkerError::Worker(message) => write!(f, "the worker answered an error: {message}"),
        }
    }
}

impl Error for WorkerError {}

/// An event the worker emitted outside a call answer, handed to the host's
/// event consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    /// One event on a subscribed stream.
    ObjectEvent {
        /// The stream's dotted name.
        stream: String,
        /// The event's payload.
        event: Value,
    },
    /// The worker asks for a capability it was not granted.
    PermissionRequest {
        /// The request id the answer echoes.
        id: String,
        /// The capability asked for.
        capability: Permission,
        /// The worker's reason, shown to the human deciding.
        reason: String,
    },
    /// An out-of-band worker event, carrying no plaintext.
    PluginEvent {
        /// The event's payload.
        event: Value,
    },
}

/// The kill switch one spawned worker carries: kills and reaps the process.
pub type Killer = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// The live ends of a spawned worker: its stdout, its stdin, and the kill
/// switch. The killer is called on shutdown and on death, always after the
/// polite `plugin_shutdown` ask, so an unresponsive worker cannot outlive
/// its host.
pub struct WorkerPipes {
    /// The worker's stdout, buffered for line reads.
    pub reader: Box<dyn AsyncBufRead + Unpin + Send>,
    /// The worker's stdin.
    pub writer: Box<dyn AsyncWrite + Unpin + Send>,
    /// Kills and reaps the process.
    pub killer: Killer,
}

impl fmt::Debug for WorkerPipes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerPipes").finish_non_exhaustive()
    }
}

/// How the host spawns one worker incarnation. The real path is
/// [`crate::WorkerHost::spawn_process`]; tests wire scripted in-memory
/// workers.
pub type SpawnFn = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = std::io::Result<WorkerPipes>> + Send>> + Send + Sync,
>;

/// How one worker is run: what to spawn, where its private data lives, and
/// the supervision knobs.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// The worker executable.
    pub program: PathBuf,
    /// Its arguments.
    pub args: Vec<String>,
    /// The worker's private data directory, handed over in `plugin_init`.
    pub data_dir: PathBuf,
    /// The handshake deadline (the plan's 5s).
    pub init_timeout: Duration,
    /// The restart budget (the plan's 3).
    pub restart_max: u32,
    /// The backoff base; the schedule is linear, capped at four seconds.
    pub backoff_base: Duration,
}
