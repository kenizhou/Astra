// SPDX-License-Identifier: GPL-3.0-only
//! The sidecar relay: owns one Node sidecar process connection. It spawns
//! through the injected [`SpawnFn`], runs the init-to-ready handshake,
//! pumps stdout lines into [`AgentEventSink`](super::AgentEventSink)
//! emissions, routes `done` and `error` envelopes to pending prompts, and
//! lands the sidecar's `llm_call` rows in the ledger. Death detection
//! (reader EOF) and the restart policy live in [`super::supervisor`]; the
//! relay only delegates there.
//!
//! The line routing lives in [`runtime`] and the send paths (prompt,
//! abort, answers, drains, restart, shutdown) in [`messaging`], split for
//! the file-size gate the way the kylins relay splits its own.

use std::{
    collections::BTreeMap,
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Weak},
    time::Duration,
};

use astra_ai::AiStore;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::{watch, Mutex};

use super::{
    node::{BoxReader, BoxWriter, ProcessIo, SpawnFn},
    permissions::PermissionBridge,
    protocol::{InCommand, ModelConfig, PermissionPolicy},
    AgentEventSink, AgentStreamEvent, StreamPayload,
};

mod messaging;
mod runtime;

/// How long [`SidecarRelay::ensure_started`] waits for the ready
/// handshake.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything needed to (re)spawn and initialise the sidecar. `spawn` is
/// the test seam: production wires [`super::spawn_sidecar`], tests inject
/// one that runs the stub script.
#[derive(Clone)]
pub(crate) struct SidecarConfig {
    /// The validated node binary.
    pub(crate) node_path: PathBuf,
    /// The sidecar bundle script.
    pub(crate) script: PathBuf,
    /// The app data directory, delivered on init and injected into the
    /// spawn environment.
    pub(crate) data_dir: PathBuf,
    /// The resolved astra-cli path; the empty string means unresolved.
    pub(crate) cli_path: String,
    /// The harness to run: `pi` or `mock`.
    pub(crate) harness: String,
    /// The mock transcript fixture; the mock harness only.
    pub(crate) mock_fixture: Option<String>,
    /// Tier name to endpoint config, delivered in band on init.
    pub(crate) model_configs: BTreeMap<String, ModelConfig>,
    /// The permission policy table, delivered on init.
    pub(crate) policy: BTreeMap<String, PermissionPolicy>,
    /// How many consecutive failed restarts the supervisor allows before
    /// the Lost state.
    pub(crate) restart_max: u32,
    /// The linear backoff base in milliseconds, capped at
    /// [`super::BACKOFF_CAP_MS`].
    pub(crate) backoff_base_ms: u64,
    /// Set when node or script resolution failed at build time: the relay
    /// still constructs, but the first start reports this typed error so
    /// the UI can render setup guidance by prefix.
    pub(crate) config_error: Option<String>,
    /// The process factory.
    pub(crate) spawn: SpawnFn,
}

impl std::fmt::Debug for SidecarConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The model configs carry keys; the shape is the whole safe description.
        f.debug_struct("SidecarConfig")
            .field("node", &self.node_path)
            .field("script", &self.script)
            .field("harness", &self.harness)
            .field("restart_max", &self.restart_max)
            .field("backoff_base_ms", &self.backoff_base_ms)
            .field("config_error", &self.config_error)
            .finish_non_exhaustive()
    }
}

/// The observable lifecycle state of the sidecar connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RelayState {
    /// Never started, or deliberately stopped.
    Down,
    /// Spawned; waiting for the ready envelope.
    Starting,
    /// The handshake completed.
    Ready,
    /// The connection died; a restart is scheduled for attempt `attempt`.
    Restarting {
        /// The restart this schedule belongs to.
        attempt: u32,
    },
    /// The restart budget was exhausted; only a fresh start recovers.
    Lost,
}

impl RelayState {
    /// The state word the status command reports.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Restarting { .. } => "restarting",
            Self::Lost => "lost",
        }
    }
}

/// One in-flight prompt: its session and correlation id, the pair every
/// streamed event and ledger row of the turn carries.
struct PendingPrompt {
    session_id: String,
    correlation: String,
}

/// The mutable connection state, under the one lock the write path holds.
/// `pub(crate)` fields: the supervisor owns the death bookkeeping inside.
pub(crate) struct RelayInner {
    /// The sidecar's stdin, present while the connection lives.
    pub(crate) writer: Option<BoxWriter>,
    /// The process kill hook.
    pub(crate) killer: Option<Box<dyn super::node::AgentKiller>>,
    /// The consecutive failed restarts counted.
    pub(crate) attempts: u32,
    /// Bumped per spawn. Read pumps carry their generation, and a report
    /// from a stale pump is rejected by the supervisor.
    pub(crate) generation: u64,
}

/// The prompt and command id counter.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// One relay over one sidecar process, shared by every command.
pub(crate) struct SidecarRelay {
    config: SidecarConfig,
    sink: Arc<dyn AgentEventSink>,
    store: Arc<AiStore>,
    bridge: PermissionBridge,
    inner: Arc<Mutex<RelayInner>>,
    /// In-flight prompts by id. Its own lock: the writer lock is held
    /// across the write await, so a shared lock would deadlock routing.
    pending: Mutex<HashMap<String, PendingPrompt>>,
    /// In-flight commands by id, for the same reason.
    commands:
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>>>,
    state_tx: watch::Sender<RelayState>,
    /// Single-flight guard for ensure_started: two concurrent callers must
    /// not both spawn.
    start_lock: Mutex<()>,
    /// The self reference the send paths need for ensure_started.
    me: Weak<Self>,
}

impl std::fmt::Debug for SidecarRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SidecarRelay")
            .field("state", &self.state_tx.borrow())
            .finish_non_exhaustive()
    }
}

/// The next host-assigned id, shared by prompts and commands.
fn next_id(prefix: &str) -> String {
    format!("{prefix}-{}", NEXT_ID.fetch_add(1, Ordering::SeqCst))
}

impl SidecarRelay {
    /// Builds the relay in the Down state; nothing is spawned until the
    /// first prompt or command asks for a ready sidecar.
    pub(crate) fn new(
        config: SidecarConfig,
        sink: Arc<dyn AgentEventSink>,
        store: Arc<AiStore>,
    ) -> Arc<Self> {
        let (state_tx, _rx) = watch::channel(RelayState::Down);
        let bridge = PermissionBridge::new(Arc::clone(&store));
        Arc::new_cyclic(|me| Self {
            config,
            sink,
            store,
            bridge,
            inner: Arc::new(Mutex::new(RelayInner {
                writer: None,
                killer: None,
                attempts: 0,
                generation: 0,
            })),
            pending: Mutex::new(HashMap::new()),
            commands: Mutex::new(HashMap::new()),
            state_tx,
            start_lock: Mutex::new(()),
            me: me.clone(),
        })
    }

    /// The current lifecycle state.
    pub(crate) fn status(&self) -> RelayState {
        self.state_tx.borrow().clone()
    }

    /// The restart attempts counted so far (the tests' observability).
    #[cfg(test)]
    pub(crate) async fn attempts(&self) -> u32 {
        self.inner.lock().await.attempts
    }

    /// The live generation counter (the tests' observability).
    #[cfg(test)]
    pub(crate) async fn generation(&self) -> u64 {
        self.inner.lock().await.generation
    }

    /// Ensures the sidecar is Ready, starting it when Down or Lost. Waits
    /// up to [`READY_TIMEOUT`] for the handshake; the read pump completes
    /// it.
    ///
    /// # Errors
    /// The config error when node or the script never resolved, or a
    /// timeout message when the ready envelope never arrived.
    pub(crate) async fn ensure_started(self: &Arc<Self>) -> Result<(), String> {
        match self.status() {
            RelayState::Ready => return Ok(()),
            RelayState::Starting | RelayState::Restarting { .. } => {}
            RelayState::Down | RelayState::Lost => {
                let _guard = self.start_lock.lock().await;
                if matches!(self.status(), RelayState::Down | RelayState::Lost) {
                    self.inner.lock().await.attempts = 0;
                    self.start_once().await?;
                }
            }
        }
        let mut rx = self.state_tx.subscribe();
        let wait = async {
            loop {
                if matches!(*rx.borrow(), RelayState::Ready) {
                    return Ok(());
                }
                if rx.changed().await.is_err() {
                    return Err("the agent relay's state channel closed".to_string());
                }
            }
        };
        match tokio::time::timeout(READY_TIMEOUT, wait).await {
            Ok(result) => result,
            Err(_) => Err("the agent sidecar never reached ready".to_string()),
        }
    }

    /// Spawns once and writes the init envelope. Does not wait for Ready:
    /// the read pump flips the state when the ready envelope arrives.
    /// Failures route through the supervisor's death path.
    ///
    /// # Errors
    /// The config error, or the spawn or init write failure.
    pub(crate) async fn start_once(self: &Arc<Self>) -> Result<(), String> {
        if let Some(err) = &self.config.config_error {
            self.state_tx.send_replace(RelayState::Down);
            return Err(err.clone());
        }
        let io = match (self.config.spawn)() {
            Ok(io) => io,
            Err(e) => {
                let generation = self.inner.lock().await.generation;
                self.on_connection_dead(generation).await;
                return Err(e);
            }
        };
        let ProcessIo {
            writer,
            reader,
            killer,
        } = io;
        let generation = {
            let mut inner = self.inner.lock().await;
            inner.generation += 1;
            inner.writer = Some(writer);
            inner.killer = Some(killer);
            inner.generation
        };
        self.spawn_read_pump(reader, generation);
        let config = self.config.clone();
        let init = InCommand::Init {
            harness: config.harness,
            cli_path: config.cli_path,
            data_dir: config.data_dir.to_string_lossy().into_owned(),
            mock_fixture: config.mock_fixture,
            // A configured tier is delivered; an empty table is omitted
            // entirely (the protocol's skip rule), never sent as an empty
            // object the pi harness would read as no model.
            model_configs: (!config.model_configs.is_empty()).then_some(config.model_configs),
            policy: Some(config.policy),
        };
        if let Err(e) = self.write_command(&init).await {
            self.on_connection_dead(generation).await;
            return Err(e);
        }
        self.state_tx.send_replace(RelayState::Starting);
        Ok(())
    }

    /// The read pump: one task per spawn, carrying its generation. It holds
    /// the relay weakly: the pump, the relay's stdin, and the child form a
    /// cycle otherwise (the pump waits on the child's stdout, the child on
    /// the relay's stdin), and the weak reference is what lets dropping the
    /// relay close the pipe and take the child down with it. EOF and read
    /// errors report the death; the supervisor rejects reports from a pump
    /// whose generation has moved on.
    fn spawn_read_pump(self: &Arc<Self>, reader: BoxReader, generation: u64) {
        let relay = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut lines = reader.lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let Some(relay) = relay.upgrade() else {
                            return;
                        };
                        relay.handle_line(&line, generation).await;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        log::warn!("agent: the sidecar read failed: {e}");
                        break;
                    }
                }
            }
            if let Some(relay) = relay.upgrade() {
                relay.on_connection_dead(generation).await;
            }
        });
    }

    /// The death entry point; the backoff and Lost logic live in the
    /// supervisor. Boxed to break the auto-trait cycle through the
    /// supervisor's spawned restart.
    fn on_connection_dead(
        self: &Arc<Self>,
        generation: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(super::supervisor::handle_connection_dead(self, generation))
    }

    /// Writes one command envelope as a JSONL line.
    async fn write_command(&self, command: &InCommand) -> Result<(), String> {
        let mut line = serde_json::to_string(command)
            .map_err(|e| format!("the agent command serialise failed: {e}"))?;
        line.push('\n');
        let mut inner = self.inner.lock().await;
        match inner.writer.as_mut() {
            Some(writer) => writer
                .write_all(line.as_bytes())
                .await
                .map_err(|e| format!("the agent sidecar write failed: {e}")),
            None => Err("the agent sidecar is not running".to_string()),
        }
    }

    /// The live correlation id of a session's in-flight prompt, for the
    /// events and asks that arrive without one of their own.
    async fn correlation_of(&self, session_id: &str) -> Option<String> {
        self.pending
            .lock()
            .await
            .values()
            .find(|p| p.session_id == session_id)
            .map(|p| p.correlation.clone())
    }

    /// The correlation id of any in-flight prompt, for the permission asks
    /// that name no session: SPA0 chat is one session, so there is at most
    /// one live turn per ask.
    async fn any_correlation(&self) -> Option<String> {
        self.pending
            .lock()
            .await
            .values()
            .next()
            .map(|p| p.correlation.clone())
    }

    /// Emits one streamed event with the turn's correlation id.
    fn emit_event(&self, session_id: &str, correlation: Option<String>, event: StreamPayload) {
        self.sink.emit_agent_event(AgentStreamEvent {
            session_id: session_id.to_owned(),
            correlation,
            event,
        });
    }

    /// The config, for the supervisor's backoff and budget decisions.
    pub(crate) fn config(&self) -> &SidecarConfig {
        &self.config
    }

    /// The sink, for the supervisor's Lost emission.
    pub(crate) fn sink(&self) -> &Arc<dyn AgentEventSink> {
        &self.sink
    }

    /// The state channel, for the supervisor's transitions.
    pub(crate) fn state_tx(&self) -> &watch::Sender<RelayState> {
        &self.state_tx
    }

    /// The inner state, for the supervisor's generation and budget bookkeeping.
    pub(crate) fn inner(&self) -> &Arc<Mutex<RelayInner>> {
        &self.inner
    }
}
