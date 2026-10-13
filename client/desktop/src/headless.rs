// SPDX-License-Identifier: GPL-3.0-only
//! The headless slice runner: `astra-desktop --headless-slice <add|list>` drives the
//! SP1 vertical slice (dial, certificate acceptance, add, sync, tree, restart) without
//! a window, for `scripts/dev/slice-check.sh`.
//!
//! Shape decision: the headless run does not start the Tauri shell. Nothing in the
//! slice needs it: the flow drives the same command-surface functions the webview
//! invokes ([`imap_test_with`], [`add_account_with`], [`dispatch_intent`],
//! [`mailbox_list_of`]) directly, and the host state is
//! assembled by the same [`assemble_state`] the windowed boot uses, fed a logging
//! event sink instead of the Tauri emitter and a data directory from the environment
//! instead of the shell's app data dir. Running `tauri::Builder` with the window
//! skipped would carry an event system whose only listener is a webview that never
//! exists; the standalone shape is the honest one.
//!
//! The two phases share one `ASTRA_SLICE_DATA_DIR`. `add` starts from an empty store,
//! walks the certificate acceptance path (the first dial is refused, the harness
//! fingerprint accepted through the same `accepted_certificate` field a person's
//! acceptance arrives on), adds the account, dispatches `refresh_mail`, polls the
//! mailbox list until the folder tree holds the folders the harness seeds, and prints
//! the tree as JSON on stdout. `list` restarts over the same directory, asserts the
//! tree came back from the keyring and the engine store without re-adding, prints it,
//! and takes the stored credential back out so the next run starts clean. Progress
//! narration goes to stderr; stdout carries only the JSON, which is what the script
//! asserts on. A logger is installed before anything runs, so the boot and dial
//! paths' records land in the same stderr narration: its level comes from
//! `ASTRA_LOG` and is Error unless that names another.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use astra_viewmodel::MailboxListSnapshot;
use engine_api::AccountId;

use crate::{
    boot::assemble_state,
    commands::{
        add_account_with, dispatch_intent, imap_test_with, mailbox_list_of, ImapTestOutcome,
    },
    observer::{DebouncedObserver, ObserverBridge, SurfaceChanged, SurfaceEventSink},
    AppState, HostProvider,
};

/// How long a phase waits for the folder tree to complete before failing: the first
/// sync of the harness's small mailboxes is quick, and anything slower is a fault.
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// How often the poll pulls the mailbox-list snapshot. One debounce window: cheaper
/// than the rate limiter ever lets a surface repaint.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long exiting waits for in-flight sync before it stops waiting, mirroring the
/// windowed host's exit bound.
const STOP_SYNC_BOUND: Duration = Duration::from_secs(5);

/// The folders the harness seeds for `astra1@example.test`, which the slice asserts
/// the tree holds: the reserved INBOX and the custom folder.
const EXPECTED_FOLDERS: [&str; 2] = ["INBOX", "Projects"];

/// The headless host's event sink: there is no webview to emit to, so surface
/// changes go to the logger at debug level, which `ASTRA_LOG` turns on.
#[derive(Debug)]
struct LoggingEventSink;

impl SurfaceEventSink for LoggingEventSink {
    fn surface_changed(&self, change: SurfaceChanged) {
        log::debug!("headless: the {:?} surface changed", change.surface);
    }
}

/// The headless run's draft-pool sink: one line per landed row, in place
/// of the windowed host's webview event.
#[derive(Debug)]
struct LoggingDraftSink;

impl astra_app::DraftPoolSink for LoggingDraftSink {
    fn draft_pool_landed(&self, message: astra_viewmodel::MessageKey) {
        log::debug!(
            "headless: a draft-pool row landed for account {}",
            message.account.as_str(),
        );
    }
}

/// The headless run's logger: one line per record on stderr, which already
/// carries the run's narration. Hand-rolled because the headless slice is a
/// diagnostic tool that pulls no logging framework behind it.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// One `ASTRA_LOG` value as a level filter, Error on anything the log crate
/// does not name: quiet by default, loud on request.
fn level_from(text: &str) -> log::LevelFilter {
    text.parse().unwrap_or(log::LevelFilter::Error)
}

/// Installs the stderr logger with the level `ASTRA_LOG` names (Error when the
/// variable is unset), so a failed boot or dial leaves its records in the
/// run's output.
fn install_logger() {
    static LOGGER: StderrLogger = StderrLogger;
    let level =
        std::env::var("ASTRA_LOG").map_or(log::LevelFilter::Error, |text| level_from(&text));
    // A refusal means a logger is already installed; the run keeps that one.
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
    }
}

/// The harness connection details and the slice's own settings, from the environment
/// `scripts/dev/harness.sh env` names plus the slice's data directory.
struct SliceEnv {
    /// The IMAP server's host.
    imap_host: String,
    /// The IMAP server's port.
    imap_port: String,
    /// The account's login (its full email address).
    username: String,
    /// The account's password.
    password: String,
    /// The directory the engine store, the account index, and the preferences live
    /// in: a scratch directory the script owns, never the real app data dir.
    data_dir: PathBuf,
    /// How long a phase may wait for the folder tree.
    timeout: Duration,
}

impl SliceEnv {
    /// Reads the environment, defaulting the harness values to the local harness and
    /// refusing to run without a data directory: the headless slice never touches the
    /// real profile.
    fn from_env() -> Result<Self, String> {
        let data_dir = std::env::var_os("ASTRA_SLICE_DATA_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "ASTRA_SLICE_DATA_DIR is not set: the headless slice runs over a scratch \
                 directory, never the real app data directory"
                    .to_owned()
            })?;
        let timeout = std::env::var("ASTRA_SLICE_TIMEOUT_SECS")
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        Ok(Self {
            imap_host: env_or("ASTRA_HARNESS_IMAP_HOST", "127.0.0.1"),
            imap_port: env_or("ASTRA_HARNESS_IMAP_PORT", "2143"),
            username: env_or("ASTRA_HARNESS_IMAP_USER", "astra1@example.test"),
            password: env_or("ASTRA_HARNESS_IMAP_PASSWORD", "astra-harness"),
            data_dir,
            timeout: Duration::from_secs(timeout),
        })
    }
}

/// One environment variable, or `default`.
fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

/// The setup the webview's setup form would submit for the harness account, before
/// any certificate has been accepted.
fn setup_for(env: &SliceEnv) -> astra_account::AccountSetup {
    astra_account::AccountSetup {
        imap_host: format!("{}:{}", env.imap_host, env.imap_port),
        username: env.username.clone(),
        credential: astra_account::SetupCredential::Password(env.password.clone()),
        smtp_host: None,
        caldav_base_url: None,
        imap_security: astra_account::ConnectionSecurity::ImplicitTls,
        smtp_security: astra_account::ConnectionSecurity::ImplicitTls,
        accepted_certificate: None,
    }
}

/// The headless counterpart of the `imap_test_connection` command's dial: one
/// connection test through the runtime's driver thread.
fn imap_test_dial(
    state: &AppState,
) -> impl FnOnce(&astra_account::AccountConfig) -> Result<(), astra_account::AccountError> + '_ {
    |config: &astra_account::AccountConfig| {
        state.runtime.block_on(|| async {
            let id = config
                .account_id()
                .map_err(|err| astra_account::AccountError::Mailbox(err.to_string()))?;
            astra_account::connect_mail_providers(
                &astra_account::ImapConnections::new(),
                config,
                &id,
            )
            .await
            .map(|_| ())
        })
    }
}

/// The headless counterpart of the `add_account` command's dial: connect and hand
/// back the pool the providers drew on, so later binds share it.
fn add_dial(
    state: &AppState,
) -> impl FnOnce(
    &astra_account::AccountConfig,
    &AccountId,
) -> Result<
    (
        Vec<HostProvider>,
        std::sync::Arc<astra_account::ImapConnections>,
    ),
    astra_account::AccountError,
> + '_ {
    |config: &astra_account::AccountConfig, id: &AccountId| {
        let connections = astra_account::ImapConnections::new();
        let providers = state
            .runtime
            .block_on(|| astra_account::connect_mail_providers(&connections, config, id));
        providers.map(|providers| (providers, connections))
    }
}

/// Boots the host state over the slice's data directory, with the logging sinks in
/// place of the Tauri emitters.
fn assemble_for(env: &SliceEnv) -> Result<AppState, String> {
    let observer: Arc<dyn astra_app::AppObserver> = Arc::new(DebouncedObserver::new(
        ObserverBridge::new(Box::new(LoggingEventSink)),
    ));
    let agent_sink: Arc<dyn crate::agent::AgentEventSink> = Arc::new(crate::agent::LogAgentSink);
    let draft_sink: Arc<dyn astra_app::DraftPoolSink> = Arc::new(LoggingDraftSink);
    // The headless host has no browser and no deep-link capture, so its
    // OAuth surface stands over the offline stand-ins: no sign-in can
    // begin there, which nothing in the slice tries.
    let oauth = crate::oauth::commands::offline_surface();
    assemble_state(&env.data_dir, observer, agent_sink, draft_sink, oauth)
        .map_err(|err| format!("the headless host could not boot: {err}"))
}

/// Which of `expected` no account's tree holds: the slice's assertion, as the diff a
/// failure prints. Any account counts; the slice adds exactly one, but the assertion
/// stays honest if the store ever holds more.
fn tree_diff(snapshot: &MailboxListSnapshot, expected: &[&str]) -> Vec<String> {
    expected
        .iter()
        .filter(|name| {
            !snapshot
                .account_folders
                .iter()
                .any(|account| account.folders.iter().any(|folder| folder.name == **name))
        })
        .map(|name| format!("missing folder: {name}"))
        .collect()
}

/// Pulls `probe` until its snapshot holds every expected folder, or the timeout
/// passes, in which case the last snapshot and its diff come back for the failure
/// report.
fn poll_until_complete<F>(
    timeout: Duration,
    interval: Duration,
    mut probe: F,
) -> Result<MailboxListSnapshot, Box<(MailboxListSnapshot, Vec<String>)>>
where
    F: FnMut() -> MailboxListSnapshot,
{
    let deadline = Instant::now() + timeout;
    let mut snapshot = probe();
    loop {
        let diff = tree_diff(&snapshot, &EXPECTED_FOLDERS);
        if diff.is_empty() {
            return Ok(snapshot);
        }
        if Instant::now() >= deadline {
            return Err(Box::new((snapshot, diff)));
        }
        std::thread::sleep(interval);
        snapshot = probe();
    }
}

/// Waits for the folder tree and renders it as the JSON the script asserts on.
fn wait_for_tree(state: &AppState, env: &SliceEnv) -> Result<String, String> {
    eprintln!("slice: polling the mailbox list for the folder tree");
    match poll_until_complete(env.timeout, POLL_INTERVAL, || mailbox_list_of(state)) {
        Ok(snapshot) => serde_json::to_string_pretty(&snapshot).map_err(|err| err.to_string()),
        Err(last) => {
            // The tree as it last stood goes to stdout beside the failure, so the
            // report shows the diff against something.
            let (snapshot, diff) = *last;
            if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
                println!("{json}");
            }
            Err(format!("the folder tree never completed: {diff:?}"))
        }
    }
}

/// The `add` phase: connection test, certificate acceptance, add, refresh, poll.
fn run_add(env: &SliceEnv) -> Result<String, String> {
    let state = assemble_for(env)?;
    let outcome = add_flow(&state, env);
    state.runtime.shutdown_timeout(STOP_SYNC_BOUND);
    outcome
}

/// The body of the `add` phase, over the booted state.
fn add_flow(state: &AppState, env: &SliceEnv) -> Result<String, String> {
    eprintln!("slice: testing the connection (a first dial of the harness is refused)");
    let mut setup = setup_for(env);
    let first =
        imap_test_with(to_value(&setup)?, imap_test_dial(state)).map_err(|err| err.to_string())?;
    match first {
        ImapTestOutcome::Ok { .. } => {
            eprintln!("slice: the server certificate verified; no exception to accept");
        }
        ImapTestOutcome::RejectedCertificate { certificate, .. } => {
            eprintln!(
                "slice: accepting the rejected certificate for {} (sha256 {})",
                certificate.server_name, certificate.sha256,
            );
            let accepted = astra_account::CertificateException::accepted(
                &certificate.server_name,
                &certificate.sha256,
            )
            .ok_or_else(|| {
                format!(
                    "the rejected certificate's fingerprint could not be read: {}",
                    certificate.sha256
                )
            })?;
            setup.accepted_certificate = Some(accepted);
            match imap_test_with(to_value(&setup)?, imap_test_dial(state))
                .map_err(|err| err.to_string())?
            {
                ImapTestOutcome::Ok { .. } => {}
                other => {
                    return Err(format!(
                        "the dial with the accepted certificate did not connect: {other:?}"
                    ));
                }
            }
        }
        ImapTestOutcome::Failed { message } => {
            return Err(format!("the connection test failed: {message}"));
        }
    }

    let config_toml = astra_account::build_config_toml(&setup).map_err(|err| err.to_string())?;
    eprintln!("slice: adding the account");
    let added =
        add_account_with(state, &config_toml, add_dial(state)).map_err(|err| err.to_string())?;
    eprintln!(
        "slice: account {} added; dispatching refresh_mail",
        added.email
    );
    dispatch_intent(state, to_value(&astra_app::Intent::RefreshMail)?)
        .map_err(|err| err.to_string())?;
    wait_for_tree(state, env)
}

/// The `list` phase: restart over the same data directory, no re-add.
fn run_list(env: &SliceEnv) -> Result<String, String> {
    let state = assemble_for(env)?;
    let outcome = list_flow(&state, env);
    state.runtime.shutdown_timeout(STOP_SYNC_BOUND);
    outcome
}

/// The body of the `list` phase, over the restarted state.
fn list_flow(state: &AppState, env: &SliceEnv) -> Result<String, String> {
    let ids = state.connections.ids();
    if ids.is_empty() {
        return Err(
            "no stored account survived the restart: the add phase's store did not \
             persist"
                .to_owned(),
        );
    }
    eprintln!(
        "slice: the restart restored {} account(s) from the keyring and the engine store",
        ids.len(),
    );
    let json = wait_for_tree(state, env)?;
    // The slice owns its credential: take it back out so the machine is as it was.
    // A refusal is reported, not fatal: the assertion has passed, and the next add
    // overwrites the entry anyway.
    for id in &ids {
        if let Err(err) = state.credential_store.delete(id) {
            eprintln!("slice: the stored credential could not be taken back: {err}");
        }
    }
    Ok(json)
}

/// One value as the JSON the command surface takes.
fn to_value<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|err| err.to_string())
}

/// Prints the headless slice's usage.
fn print_usage() {
    eprintln!("usage: astra-desktop --headless-slice <add|list>");
    eprintln!("  add   dial the harness account, accept its certificate, add the");
    eprintln!("        account, dispatch refresh_mail, poll the tree, print it as JSON");
    eprintln!("  list  restart over the same ASTRA_SLICE_DATA_DIR and print the tree");
    eprintln!("        the keyring and the engine store persisted, without re-adding");
    eprintln!("env: ASTRA_HARNESS_IMAP_HOST, ASTRA_HARNESS_IMAP_PORT,");
    eprintln!("     ASTRA_HARNESS_IMAP_USER, ASTRA_HARNESS_IMAP_PASSWORD,");
    eprintln!("     ASTRA_SLICE_DATA_DIR (required), ASTRA_SLICE_TIMEOUT_SECS,");
    eprintln!("     ASTRA_LOG (stderr log level, Error by default)");
}

/// The function one phase runs as: `run_add` or `run_list`.
type PhaseRun = fn(&SliceEnv) -> Result<String, String>;

/// Validates the phase name into the function that runs it.
fn phase_runner(phase: &str) -> Result<PhaseRun, String> {
    match phase {
        "add" => Ok(run_add),
        "list" => Ok(run_list),
        other => Err(format!("unknown phase '{other}'")),
    }
}

/// Resolves what the run does: the phase first, the environment second, so an
/// unknown phase reports its own usage error even when the environment is
/// unset as well.
fn resolve_run(
    phase: &str,
    read_env: impl FnOnce() -> Result<SliceEnv, String>,
) -> Result<(PhaseRun, SliceEnv), String> {
    Ok((phase_runner(phase)?, read_env()?))
}

/// Runs the headless slice phase and answers the process exit code: 0 on success, 1
/// on a failed assertion (the diff is printed), 2 on a usage error.
pub fn run_headless_slice(phase: &str) -> i32 {
    install_logger();
    let (run, env) = match resolve_run(phase, SliceEnv::from_env) {
        Ok(resolved) => resolved,
        Err(message) => {
            eprintln!("slice: {message}");
            print_usage();
            return 2;
        }
    };
    match run(&env) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(message) => {
            eprintln!("slice: FAIL: {message}");
            1
        }
    }
}

#[cfg(test)]
#[path = "headless_tests.rs"]
mod tests;
