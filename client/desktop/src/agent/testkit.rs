// SPDX-License-Identifier: GPL-3.0-only
//! The agent tests' shared kit: the collecting sink every suite asserts
//! against, the scratch directories the file-backed stores live in, and
//! the stub-relay builders that run the fixture sidecar script under a real
//! Node process in a chosen mode.

use std::sync::{Arc, Mutex};

use super::{
    default_policy, protocol::ModelConfig, relay::SidecarConfig, spawn_sidecar, AgentEventSink,
    AgentStreamEvent, PendingApproval, PermissionResolved, SidecarRelay,
};
use crate::agent::node::find_node;
use astra_ai::AiStore;

/// A sink that collects every emission for the relay, supervisor, and
/// command tests to assert against.
#[derive(Default)]
pub(crate) struct CollectingSink {
    /// Every streamed turn event, in order.
    pub(crate) agent_events: Mutex<Vec<AgentStreamEvent>>,
    /// The ready handshakes.
    pub(crate) ready: Mutex<Vec<(Vec<String>, Vec<String>)>>,
    /// The lost notifications.
    pub(crate) lost: Mutex<Vec<(u32, String)>>,
    /// The permission asks, in order.
    pub(crate) requests: Mutex<Vec<PendingApproval>>,
    /// The settled approvals, in order.
    pub(crate) resolved: Mutex<Vec<PermissionResolved>>,
}

impl CollectingSink {
    /// Blocks the caller until the stream has grown to `count` events.
    pub(crate) async fn await_events(&self, count: usize) -> Vec<AgentStreamEvent> {
        for _ in 0..2000 {
            {
                let events = self.agent_events.lock().unwrap();
                if events.len() >= count {
                    return events.clone();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        self.agent_events.lock().unwrap().clone()
    }
}

impl AgentEventSink for CollectingSink {
    fn emit_agent_event(&self, event: AgentStreamEvent) {
        self.agent_events.lock().unwrap().push(event);
    }

    fn emit_sidecar_ready(&self, models: Vec<String>, harnesses: Vec<String>) {
        self.ready.lock().unwrap().push((models, harnesses));
    }

    fn emit_sidecar_lost(&self, attempts: u32, message: &str) {
        self.lost
            .lock()
            .unwrap()
            .push((attempts, message.to_string()));
    }

    fn emit_permission_request(&self, approval: PendingApproval) {
        self.requests.lock().unwrap().push(approval);
    }

    fn emit_permission_resolved(&self, resolved: PermissionResolved) {
        self.resolved.lock().unwrap().push(resolved);
    }
}

/// A scratch directory for one test's data dir and ledger store. The
/// name plus the process id names one directory for the whole test, so
/// every call inside one process reuses it; the first call for a name
/// removes any directory a recycled process id left behind, because the
/// operating system never cleans the parent.
pub(crate) fn scratch(name: &str) -> std::path::PathBuf {
    static HANDED_OUT: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let handed_out = HANDED_OUT.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let key = format!("{name}-{}", std::process::id());
    let first_for_this_process = {
        let mut held = handed_out.lock().unwrap();
        held.insert(key.clone())
    };
    let dir = std::env::temp_dir()
        .join("astra-desktop-agent-tests")
        .join(&key);
    if first_for_this_process && dir.exists() {
        std::fs::remove_dir_all(&dir).expect("the stale scratch directory removes");
    }
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// The stub sidecar script beside this module.
pub(crate) fn stub_script() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/agent/fixtures/stub-sidecar.cjs")
}

/// A relay over the stub sidecar in `mode`, with a collecting sink and a
/// file-backed store under its scratch data dir.
pub(crate) fn stub_relay(
    name: &str,
    mode: &str,
    restart_max: u32,
    backoff_base_ms: u64,
) -> (Arc<SidecarRelay>, Arc<CollectingSink>, std::path::PathBuf) {
    stub_relay_with(name, &[mode], &[], restart_max, backoff_base_ms)
}

/// A relay whose init envelope carries `model_configs`, running the stub's
/// `echo-init` mode so the test can read the delivery back.
pub(crate) fn configured_stub_relay(
    name: &str,
    model_configs: &[(String, ModelConfig)],
) -> (Arc<SidecarRelay>, Arc<CollectingSink>, std::path::PathBuf) {
    stub_relay_with(name, &["echo-init"], model_configs, 3, 50)
}

/// The rotating-spawn form: each spawn call runs the next mode in the
/// list, wrapping to the last one, so a test can exhaust a budget with
/// dying sidecars and then recover with a live one. The model configs
/// ride every init envelope the relay writes.
pub(crate) fn stub_relay_with(
    name: &str,
    modes: &[&str],
    model_configs: &[(String, ModelConfig)],
    restart_max: u32,
    backoff_base_ms: u64,
) -> (Arc<SidecarRelay>, Arc<CollectingSink>, std::path::PathBuf) {
    let data_dir = scratch(name);
    let node = find_node().expect("a node >= 22 is on PATH for the agent tests");
    let script = stub_script();
    let spawn = {
        let modes: Vec<String> = modes.iter().map(|mode| (*mode).to_string()).collect();
        let node = node.clone();
        let script = script.clone();
        let data_dir = data_dir.clone();
        let next = std::sync::atomic::AtomicUsize::new(0);
        Arc::new(move || {
            let at = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mode = modes
                .get(at)
                .or_else(|| modes.last())
                .cloned()
                .unwrap_or_default();
            let spawn = spawn_sidecar(node.clone(), script.clone(), data_dir.clone(), vec![mode]);
            spawn()
        })
    };
    let sink = Arc::new(CollectingSink::default());
    let store = Arc::new(AiStore::open(&data_dir.join("ai.sqlite")).expect("the store opens"));
    let relay = SidecarRelay::new(
        SidecarConfig {
            node_path: node,
            script,
            data_dir: data_dir.clone(),
            cli_path: String::new(),
            harness: "stub".to_string(),
            mock_fixture: None,
            model_configs: model_configs.iter().cloned().collect(),
            policy: default_policy(),
            restart_max,
            backoff_base_ms,
            config_error: None,
            spawn,
        },
        sink.clone(),
        store,
    );
    (relay, sink, data_dir)
}

/// Reads the audit rows the relay's decisions and tool results wrote, in
/// write order: the tests' own connection into the file-backed store,
/// because the store itself only appends.
pub(crate) fn audit_rows(data_dir: &std::path::Path) -> Vec<(String, String, String)> {
    let conn = rusqlite::Connection::open(data_dir.join("ai.sqlite"))
        .expect("the test store opens for reading");
    let mut statement = conn
        .prepare("SELECT correlation_id, verb, outcome FROM audit_log ORDER BY rowid")
        .expect("the audit query prepares");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("the audit query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("the audit rows read");
    rows
}

/// Reads the ledger rows the sidecar's llm_call envelopes wrote, in write
/// order: correlation, model, tier, use case, and the token pair.
pub(crate) fn llm_rows(
    data_dir: &std::path::Path,
) -> Vec<(String, String, String, String, u64, u64)> {
    let conn = rusqlite::Connection::open(data_dir.join("ai.sqlite"))
        .expect("the test store opens for reading");
    let mut statement = conn
        .prepare(
            "SELECT correlation_id, model, tier, use_case, prompt_tokens, completion_tokens
             FROM llm_calls ORDER BY rowid",
        )
        .expect("the ledger query prepares");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, u64>(5)?,
            ))
        })
        .expect("the ledger query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("the ledger rows read");
    rows
}

#[cfg(test)]
mod tests {
    use super::scratch;

    #[test]
    fn scratch_wipes_a_directory_a_recycled_pid_left_behind() {
        let stale_dir = std::env::temp_dir()
            .join("astra-desktop-agent-tests")
            .join(format!("scratch-recycled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&stale_dir);
        std::fs::create_dir_all(&stale_dir).expect("the stale directory creates");
        let stale = stale_dir.join("stub-captures.jsonl");
        std::fs::write(&stale, "stale\n").expect("the stale file writes");
        let dir = scratch("scratch-recycled");
        assert_eq!(dir, stale_dir, "the same pid names the same directory");
        assert!(
            !stale.exists(),
            "a directory an earlier run left behind starts empty"
        );
    }

    #[test]
    fn scratch_hands_one_process_the_same_directory_every_time() {
        let dir = scratch("scratch-reuse");
        let kept = dir.join("kept");
        std::fs::write(&kept, "here").expect("the kept file writes");
        let again = scratch("scratch-reuse");
        assert_eq!(again, dir, "one process reuses its own directory");
        assert!(kept.exists(), "a later call never wipes the live directory");
    }
}
