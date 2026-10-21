// SPDX-License-Identifier: GPL-3.0-only
//! The worker host: one out-of-process worker per activated plugin, spoken to
//! over the envelope family's JSONL stdio, supervised with the sidecar's
//! discipline.
//!
//! The host owns the whole lifecycle: lazy spawn on the first start, the
//! `plugin_init` / `plugin_ready` handshake with a pinned version check, call
//! dispatch through the router (a call the router refuses never reaches the
//! pipe), event subscription bookkeeping, and shutdown as ask-then-kill so an
//! unresponsive worker cannot outlive its host. A worker's death fails its
//! pending calls closed and drains its subscriptions; restarts mint a fresh
//! generation and a fresh handle table, so a handle from before a restart is
//! stale forever after.

mod pump;
mod supervision;
mod types;

use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
    time::Duration,
};

use serde_json::Value;
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite},
    sync::{mpsc, oneshot, watch, Mutex},
};

pub use types::{
    Killer, SpawnFn, WorkerConfig, WorkerError, WorkerEvent, WorkerPipes, WorkerState,
};

use crate::{
    HandleKind, HandleTable, HostMessage, ObjectRef, Permission, PluginManifest, Router,
    WorkerMessage, AOM_VERSION, PLUGIN_API_LEVEL,
};

/// The shared state the host, the pump, and supervision all touch.
pub(crate) struct Shared {
    pub(crate) config: WorkerConfig,
    pub(crate) manifest: PluginManifest,
    pub(crate) router: Router,
    pub(crate) spawn: SpawnFn,
    pub(crate) state_tx: watch::Sender<WorkerState>,
    pub(crate) events_tx: mpsc::UnboundedSender<WorkerEvent>,
    pub(crate) inner: Mutex<Inner>,
}

impl Shared {
    pub(crate) fn state(&self) -> WorkerState {
        self.state_tx.borrow().clone()
    }

    pub(crate) fn set_state(&self, state: WorkerState) {
        self.state_tx.send_replace(state);
    }
}

pub(crate) struct Inner {
    pub(crate) generation: u64,
    pub(crate) attempts: u32,
    pub(crate) writer: Option<Box<dyn AsyncWrite + Unpin + Send>>,
    pub(crate) killer: Option<Killer>,
    pub(crate) pump: Option<tokio::task::JoinHandle<()>>,
    pub(crate) pending: HashMap<String, oneshot::Sender<Result<Value, WorkerError>>>,
    pub(crate) subscriptions: HashSet<String>,
    pub(crate) call_serial: u64,
    pub(crate) table: HandleTable,
}

/// The host of one plugin's worker. Clone it to share; every clone drives the
/// same worker.
#[derive(Clone)]
pub struct WorkerHost {
    shared: Arc<Shared>,
    events_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<WorkerEvent>>>>,
}

impl fmt::Debug for WorkerHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerHost")
            .field("state", &self.shared.state())
            .finish_non_exhaustive()
    }
}

impl WorkerHost {
    /// A host for one plugin with a custom spawn path (tests, the sandboxed
    /// frame's future needs). The worker is Down until [`WorkerHost::start`].
    #[must_use]
    pub fn new(
        config: WorkerConfig,
        manifest: PluginManifest,
        grants: &[Permission],
        dev: bool,
        spawn: SpawnFn,
    ) -> Self {
        let (state_tx, _) = watch::channel(WorkerState::Down);
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let router = Router::new(manifest.clone(), grants, dev);
        let table = HandleTable::new(manifest.id.clone(), 0);
        Self {
            shared: Arc::new(Shared {
                config,
                manifest,
                router,
                spawn,
                state_tx,
                events_tx,
                inner: Mutex::new(Inner {
                    generation: 0,
                    attempts: 0,
                    writer: None,
                    killer: None,
                    pump: None,
                    pending: HashMap::new(),
                    subscriptions: HashSet::new(),
                    call_serial: 0,
                    table,
                }),
            }),
            events_rx: Arc::new(Mutex::new(Some(events_rx))),
        }
    }

    /// A host that spawns the configured program as a real process with piped
    /// stdio (stderr inherited: the worker's diagnostics land in the host's).
    #[must_use]
    pub fn spawn_process(
        config: WorkerConfig,
        manifest: PluginManifest,
        grants: &[Permission],
        dev: bool,
    ) -> Self {
        let spawn_config = config.clone();
        let spawn: SpawnFn = Arc::new(move || {
            let config = spawn_config.clone();
            Box::pin(async move { spawn_real(&config) })
        });
        Self::new(config, manifest, grants, dev, spawn)
    }

    /// The current supervision state.
    pub fn state(&self) -> WorkerState {
        self.shared.state()
    }

    /// Watches the supervision state.
    pub fn state_watch(&self) -> watch::Receiver<WorkerState> {
        self.shared.state_tx.subscribe()
    }

    /// Takes the event stream the worker emits. Once per host; the second
    /// take answers None.
    pub async fn take_events(&self) -> Option<mpsc::UnboundedReceiver<WorkerEvent>> {
        self.events_rx.lock().await.take()
    }

    /// Spawns the worker and runs the handshake, retrying per the supervision
    /// policy until Ready or Lost. A manual start on a live host is a
    /// restart: the budget resets and a fresh generation mints.
    ///
    /// # Errors
    ///
    /// Returns the error that exhausted the budget or failed fatally.
    pub async fn start(&self) -> Result<(), WorkerError> {
        {
            let mut inner = self.shared.inner.lock().await;
            inner.attempts = 0;
            inner.generation += 1;
        }
        self.shared.set_state(WorkerState::Starting);
        supervision::retry_loop(&self.shared).await
    }

    /// Mints a handle for one object in this worker's current generation. The
    /// host calls this when publishing snapshots; the plugin only ever
    /// returns the handles it was handed.
    pub async fn mint_handle(&self, kind: HandleKind, account: &str, key: &str) -> ObjectRef {
        self.shared
            .inner
            .lock()
            .await
            .table
            .mint(kind, account.to_string(), key.to_string())
    }

    /// Routes one object call through the router and the pipe and awaits its
    /// answer.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError::Refused`] when the router's gates fire,
    /// [`WorkerError::NotReady`] when the worker is not serving,
    /// [`WorkerError::Died`] when the worker dies with the call in flight,
    /// and [`WorkerError::Worker`] when the worker answered an error.
    pub async fn call(
        &self,
        handle: Option<ObjectRef>,
        method: &str,
        args: Value,
    ) -> Result<Value, WorkerError> {
        let receiver = {
            let mut inner = self.shared.inner.lock().await;
            if self.shared.state() != WorkerState::Ready || inner.writer.is_none() {
                return Err(WorkerError::NotReady);
            }
            let routed = self
                .shared
                .router
                .route(&inner.table, &handle, method, &args)
                .map_err(WorkerError::Refused)?;
            // The routed entry is validated here; the host's executor (SP3b)
            // will consume `routed.handle`'s internal key.
            let _ = routed;
            inner.call_serial += 1;
            let id = format!("c{}", inner.call_serial);
            let (sender, receiver) = oneshot::channel();
            inner.pending.insert(id.clone(), sender);
            let message = HostMessage::ObjectCall {
                id: id.clone(),
                handle,
                method: method.to_string(),
                args,
            };
            let writer = inner.writer.as_mut().expect("checked above");
            if let Err(error) = pump::write_message(writer, &message).await {
                inner.pending.remove(&id);
                let generation = inner.generation;
                drop(inner);
                supervision::handle_dead(&self.shared, generation).await;
                return Err(error);
            }
            receiver
        };
        receiver.await.map_err(|_| WorkerError::Died)?
    }

    /// Subscribes to one event stream. Death drains the subscription on the
    /// supervision path; a restarted worker starts with none.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError::NotReady`] when the worker is not serving and
    /// [`WorkerError::Io`] when the write fails.
    pub async fn subscribe(&self, stream: &str) -> Result<(), WorkerError> {
        let mut inner = self.shared.inner.lock().await;
        if self.shared.state() != WorkerState::Ready || inner.writer.is_none() {
            return Err(WorkerError::NotReady);
        }
        inner.call_serial += 1;
        let id = format!("s{}", inner.call_serial);
        let message = HostMessage::ObjectSubscribe {
            id,
            stream: stream.to_string(),
        };
        let writer = inner.writer.as_mut().expect("checked above");
        pump::write_message(writer, &message).await?;
        inner.subscriptions.insert(stream.to_string());
        Ok(())
    }

    /// Asks the worker to exit, then kills and reaps it regardless. The state
    /// lands in Down and stays: a dead pump reporting after this is stale and
    /// counts nothing.
    pub async fn shutdown(&self) {
        let (writer, killer, pump) = {
            let mut inner = self.shared.inner.lock().await;
            inner.generation += 1;
            for (_, pending) in inner.pending.drain() {
                let _ = pending.send(Err(WorkerError::Died));
            }
            inner.subscriptions.clear();
            (inner.writer.take(), inner.killer.take(), inner.pump.take())
        };
        self.shared.set_state(WorkerState::Down);
        if let Some(mut writer) = writer {
            let _ = pump::write_message(&mut *writer, &HostMessage::PluginShutdown).await;
        }
        if let Some(killer) = killer {
            killer().await;
        }
        if let Some(pump) = pump {
            pump.abort();
        }
    }

    /// The current supervisor generation (tests assert restarts mint fresh
    /// ones).
    #[cfg(test)]
    pub(crate) async fn generation(&self) -> u64 {
        self.shared.inner.lock().await.generation
    }

    /// The number of live subscriptions (tests assert death drains them).
    #[cfg(test)]
    pub(crate) async fn subscription_count(&self) -> usize {
        self.shared.inner.lock().await.subscriptions.len()
    }
}

/// Spawns the configured program with piped stdio and returns its pipes. The
/// child handle lives inside the killer so shutdown and death can reap it.
fn spawn_real(config: &WorkerConfig) -> std::io::Result<WorkerPipes> {
    let mut child = tokio::process::Command::new(&config.program)
        .args(&config.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;
    let stdout = child.stdout.take().expect("piped");
    let stdin = child.stdin.take().expect("piped");
    let child = Arc::new(Mutex::new(Some(child)));
    Ok(WorkerPipes {
        reader: Box::new(tokio::io::BufReader::new(stdout)),
        writer: Box::new(stdin),
        killer: Box::new(move || {
            Box::pin(async move {
                if let Some(mut child) = child.lock().await.take() {
                    let _ = child.kill().await;
                }
            })
        }),
    })
}

/// One spawn-and-handshake pass: spawn, write `plugin_init`, read the ready
/// inside the init timeout, check the versions and the served methods, and on
/// success install the writer, killer, pump, and a fresh handle table. The
/// reader survives the handshake (the ready line is read through a borrow, so
/// the buffer keeps anything that arrived early) and becomes the pump's input.
pub(crate) async fn spawn_and_init_once(shared: &Arc<Shared>) -> Result<(), WorkerError> {
    let pipes = (shared.spawn)()
        .await
        .map_err(|error| WorkerError::Spawn(error.to_string()))?;
    let WorkerPipes {
        mut reader,
        mut writer,
        killer,
    } = pipes;
    let init = HostMessage::PluginInit {
        manifest: shared.manifest.clone(),
        grants: shared.router.grants().iter().copied().collect(),
        api_level: PLUGIN_API_LEVEL,
        aom_version: AOM_VERSION,
        data_dir: shared.config.data_dir.to_string_lossy().into_owned(),
    };
    pump::write_message(&mut *writer, &init).await?;
    let (api_level, aom_version, methods) =
        read_ready(&mut *reader, shared.config.init_timeout).await?;
    if api_level != PLUGIN_API_LEVEL || aom_version != AOM_VERSION {
        return Err(WorkerError::VersionMismatch {
            found_api_level: api_level,
            found_aom_version: aom_version,
        });
    }
    let undeclared: Vec<String> = methods
        .iter()
        .filter(|method| !shared.manifest.methods.contains(method))
        .cloned()
        .collect();
    if !undeclared.is_empty() {
        return Err(WorkerError::UndeclaredMethods(undeclared));
    }
    let generation = {
        let mut inner = shared.inner.lock().await;
        inner.writer = Some(writer);
        inner.killer = Some(killer);
        inner.table = HandleTable::new(shared.manifest.id.clone(), inner.generation);
        inner.generation
    };
    let pump_handle = pump::spawn_pump(Arc::clone(shared), generation, reader);
    shared.inner.lock().await.pump = Some(pump_handle);
    Ok(())
}

/// Reads the one ready line the handshake allows, inside the timeout. Any
/// other first message, a malformed line, or a closed pipe is a protocol
/// failure; the timeout is [`WorkerError::InitTimeout`].
async fn read_ready(
    reader: &mut (dyn AsyncBufRead + Unpin + Send),
    timeout: Duration,
) -> Result<(u32, u32, Vec<String>), WorkerError> {
    let read = async {
        let mut lines = reader.lines();
        match lines.next_line().await {
            Ok(Some(line)) => match serde_json::from_str::<WorkerMessage>(&line) {
                Ok(WorkerMessage::PluginReady {
                    api_level,
                    aom_version,
                    methods,
                }) => Ok((api_level, aom_version, methods)),
                Ok(_) => Err(WorkerError::Protocol(
                    "the first worker message was not plugin_ready".to_string(),
                )),
                Err(error) => Err(WorkerError::Protocol(error.to_string())),
            },
            Ok(None) => Err(WorkerError::Protocol(
                "the worker exited before its ready".to_string(),
            )),
            Err(error) => Err(WorkerError::Io(error.to_string())),
        }
    };
    tokio::time::timeout(timeout, read)
        .await
        .map_err(|_| WorkerError::InitTimeout)?
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
