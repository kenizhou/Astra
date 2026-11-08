// SPDX-License-Identifier: GPL-3.0-only
//! The platform batch's live proofs over the desktop's real assembly:
//! the boot's own file logger observed end to end, and the log export
//! over the file the real launch wrote.
//!
//! The logger's install is process-global (the log crate accepts a
//! single installer), and the test binary already shares one capture
//! logger, so the real install cannot run inside the shared process:
//! a proof that called it here would race the capture for the one
//! installer slot and leave the loser's assertions empty. The honest
//! shape is a process of its own: the proof spawns this test binary
//! with `--exact` over its own test, the child performs the real
//! launch (the real `logging::install` over a scratch data dir, the
//! real `assemble_state`, the real boot platform legs), and the parent
//! asserts over the real file's bytes. The gated live IMAP harness
//! adds nothing here: the logger, the ledger, and the export observe
//! no mail server, so the proofs run offline over the real assembly.
//!
//! Two proofs of this batch live elsewhere, judged there: the
//! consented events' proof lives in the platform legs' boot suite (the
//! gate pinned both ways over the real assembly, the boot's own event
//! and the withdrawal added there rather than duplicated), and the
//! language proof's whole chain (the snapshot's language field, the
//! store's apply, the formatters following the tag) is pinned by the
//! web suites' fr-FR tests, which is the entire chain, so no live
//! shape adds an observation. The export's outcome shapes stay in the
//! diagnostics seam suite; what rides here is the source being the
//! real current file the real launch wrote.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use astra_app::{Intent, SettingsIntent};

use super::{QuietDraftSink, QuietObserver};

/// The proof's own test path, the name the parent hands the child's
/// `--exact` filter so the child runs this test alone, in a process
/// whose logger slot is free.
const PROOF_TEST: &str = "boot::tests::platform_proofs::the_real_boot_s_logger_observed_end_to_end";

/// The data directory the parent names for the child's launch.
const LAUNCH_DIR: &str = "ASTRA_PLATFORM_PROOF_DIR";

/// The arm the parent names for the child's launch: set when the
/// launch flips the Diagnostics toggle before it ends.
const LAUNCH_FLIP: &str = "ASTRA_PLATFORM_PROOF_FLIP";

/// The level probe's message, the one line the proof itself logs
/// through the real facade: its absence and presence are the level
/// gate's observation, so it names itself.
const PROBE: &str = "proof: the debug probe line";

/// A scratch data directory of the proof's own, wiped so one run's
/// launches never read an earlier run's files.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "astra-desktop-boot-proofs-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// One launch of the real boot in this process: the real install over
/// `data_dir`, the real assembly, the real boot platform legs (the
/// zone report and the boot's own product event), and the level probe.
/// `flip_debug` dispatches the Diagnostics toggle the way the settings
/// surface does, so the launch's persisted preference names the next
/// launch's level.
fn launch(data_dir: &Path, flip_debug: bool) {
    // The file logger first, before any line the boot itself logs: the
    // same ordering the windowed setup hook holds.
    crate::logging::install(data_dir);
    let (_relay, agent_sink, _data) =
        crate::agent::testkit::stub_relay("proof-launch", "echo", 3, 50);
    let state = super::super::assemble_state(
        data_dir,
        Arc::new(QuietObserver),
        agent_sink,
        Arc::new(QuietDraftSink),
        crate::oauth::wiring::offline_surface(),
    )
    .expect("the real assembly boots over the scratch store");
    let accounts = state.connections.ids().len();
    crate::platform::report_boot_zone(&state);
    state
        .app
        .record_analytics(&crate::platform::app_open_event(accounts));
    log::debug!("{PROBE}");
    if flip_debug {
        state.runtime.block_on(|| {
            let app = Arc::clone(&state.app);
            async move {
                app.dispatch(Intent::Settings(SettingsIntent::SetDebugEnabled {
                    enabled: true,
                }))
                .await
                .expect("the toggle dispatch lands");
            }
        });
    }
}

/// Spawns this test binary as one launch over `data_dir` and waits for
/// it, failing with the child's own output when the launch did not
/// answer zero or did not settle inside the bound. The child boots the
/// whole real assembly, more work than any other spawned test child,
/// so a wedged launch must fail the proof red rather than hanging the
/// run forever.
fn spawn_launch(data_dir: &Path, flip: bool) {
    /// How long one launch may take before the proof kills it.
    const LAUNCH_BOUND: std::time::Duration = std::time::Duration::from_secs(120);

    let exe = std::env::current_exe().expect("the test binary's path resolves");
    let mut command = std::process::Command::new(exe);
    command
        .args(["--exact", PROOF_TEST, "--nocapture", "--test-threads=1"])
        .env(LAUNCH_DIR, data_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if flip {
        command.env(LAUNCH_FLIP, "1");
    }
    let mut child = command.spawn().expect("the launch process spawns");
    let deadline = std::time::Instant::now() + LAUNCH_BOUND;
    let status = loop {
        match child.try_wait().expect("the launch stays askable") {
            Some(status) => break status,
            None if std::time::Instant::now() >= deadline => {
                child.kill().expect("the wedged launch is killed");
                child.wait().expect("the killed launch is reaped");
                panic!(
                    "the launch over {} did not settle inside {}:\n",
                    data_dir.display(),
                    LAUNCH_BOUND.as_secs(),
                );
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut stdout);
    }
    if let Some(mut pipe) = child.stderr.take() {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut stderr);
    }
    assert!(
        status.success(),
        "the launch over {} did not answer zero:\n{}\n{}",
        data_dir.display(),
        stdout,
        stderr,
    );
}

/// Whether one line carries the file's line shape: an epoch stamp, a
/// level word, then the message.
fn shaped(line: &str) -> bool {
    let Some((stamp, rest)) = line.split_once(' ') else {
        return false;
    };
    stamp.len() >= 10 && stamp.chars().all(|digit| digit.is_ascii_digit()) && {
        let Some((level, _)) = rest.split_once(": ") else {
            return false;
        };
        matches!(level, "INFO" | "WARN" | "ERROR" | "DEBUG" | "TRACE")
    }
}

/// Whether `body` carries a Windows drive letter: one ASCII letter
/// before a colon at a word boundary, the shape a path's own start has
/// (`C:` at the head of `C:\Users`), never the tail of a word like the
/// level's `INFO:`.
fn drive_letter(body: &str) -> Option<String> {
    for (at, candidate) in body.char_indices() {
        if !candidate.is_ascii_alphabetic() || !body[at + 1..].starts_with(':') {
            continue;
        }
        let before = body[..at].chars().next_back();
        if before.is_some_and(char::is_alphanumeric) {
            continue;
        }
        return Some(format!("{candidate}:"));
    }
    None
}

/// Asserts the log contract over the real written file's bytes: no
/// secret marker, no mail address, no repository path, and no drive
/// letter, in any line any launch wrote.
fn contract_holds(body: &str) {
    let lower = body.to_lowercase();
    for word in ["password", "secret", "token", "bearer"] {
        assert!(!lower.contains(word), "no secret marker crosses: {word}");
    }
    assert!(!body.contains('@'), "no mail address crosses");
    for shape in ["/home/", "src/", "client/", "vendor/", "\\"] {
        assert!(
            !body.contains(shape),
            "no repository path or path separator crosses: {shape}"
        );
    }
    if let Some(drive) = drive_letter(body) {
        panic!("no drive letter crosses: {drive}");
    }
}

/// The proof: two real launches over one data directory. The first
/// runs at the default info level (the install's own first line says
/// so, the debug probe stays out) and flips the Diagnostics toggle the
/// real way; the second launch's install reads the persisted
/// preference and runs at debug (its install line says so, the probe
/// lands), so the level change lands with the next launch exactly as
/// the toggle's wording states. Every line either launch wrote holds
/// the file's shape and the log contract, and the export over the real
/// current file writes its exact bytes under its own name.
#[test]
fn the_real_boot_s_logger_observed_end_to_end() {
    let Some(data_dir) = std::env::var_os(LAUNCH_DIR).map(PathBuf::from) else {
        parent();
        return;
    };
    launch(&data_dir, std::env::var_os(LAUNCH_FLIP).is_some());
}

/// The parent half: drives the two launches and asserts over the real
/// file's bytes between them.
fn parent() {
    let data_dir = scratch("logger");
    // The first launch runs at the default level and flips the
    // Diagnostics toggle before it ends, the way a person does while
    // the app runs.
    spawn_launch(&data_dir, true);

    let first = std::fs::read_to_string(crate::logging::file_path(&data_dir))
        .expect("the real launch's current file stands");
    let first_lines: Vec<&str> = first.lines().collect();
    assert!(
        first_lines
            .first()
            .is_some_and(|line| line.ends_with("logging: the file log stands at INFO")),
        "the install's own line is the file's first line, at the default level:\n{first}"
    );
    assert!(
        first.contains("rebuild_snapshot: "),
        "the real assembly's own narration lands under the install:\n{first}"
    );
    assert!(
        !first.contains(PROBE),
        "the debug probe stays out at the info level:\n{first}"
    );
    for line in &first_lines {
        assert!(shaped(line), "the real file's lines hold the shape: {line}");
    }
    contract_holds(&first);

    // The toggle's flip, persisted by the first launch the real way,
    // names the next launch's level.
    spawn_launch(&data_dir, false);

    let both = std::fs::read_to_string(crate::logging::file_path(&data_dir))
        .expect("the current file stands after the second launch");
    assert!(
        both.contains("logging: the file log stands at DEBUG"),
        "the next launch's install reads the persisted toggle:\n{both}"
    );
    assert_eq!(
        both.lines().filter(|line| line.contains(PROBE)).count(),
        1,
        "the debug probe lands exactly once, in the debug launch only:\n{both}"
    );
    for line in both.lines() {
        assert!(shaped(line), "the real file's lines hold the shape: {line}");
    }
    contract_holds(&both);

    export_over_the_real_file(&data_dir);
}

/// The export proof over the real current file: the bytes the command
/// writes are the file's own bytes, exact, under the log's own name.
fn export_over_the_real_file(data_dir: &Path) {
    use crate::commands::attachments::{SaveAttachmentOutcome, SaveHost};

    /// A host whose dialog answers one chosen path and whose write is
    /// the filesystem's own.
    struct Saving(PathBuf);
    impl SaveHost for Saving {
        fn ask_save_path(&self, file_name: &str) -> Option<PathBuf> {
            assert_eq!(
                file_name, "astra.log",
                "the dialog opens on the log's own name"
            );
            Some(self.0.clone())
        }
        fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
            std::fs::write(path, bytes).map_err(|err| err.to_string())
        }
    }

    let real = crate::logging::file_path(data_dir);
    let bytes = std::fs::read(&real).expect("the real file reads");
    let mut state = crate::commands_fakes::state(
        "proof-export",
        crate::commands_fakes::FakeApp::new(),
        crate::commands_fakes::FakeStore::accepting(),
    );
    state.log_path = real;
    let chosen = scratch("export").join("support-log.bin");
    let outcome = crate::commands::diagnostics::log_export_of(&state, &Saving(chosen.clone()));

    assert_eq!(
        outcome,
        SaveAttachmentOutcome::Saved {
            path: chosen.display().to_string(),
        },
        "the export answers saved over the real file"
    );
    let written = std::fs::read(&chosen).expect("the export's file stands");
    assert_eq!(
        written, bytes,
        "the export's bytes are the real current file's bytes, exact"
    );
}
