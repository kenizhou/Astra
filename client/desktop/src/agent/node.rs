// SPDX-License-Identifier: GPL-3.0-only
//! The Node runtime, the sidecar script, the astra-cli path, and the child
//! process seam: everything the relay needs to turn its config into a live
//! stdio connection. Ported from the kylins agent's node and io modules.
//!
//! The spawn environment is an allowlist, not a scrub list: `PATH` plus
//! `ASTRA_DATA_DIR` and nothing else, so no `ASTRA_LLM_*` variable (the
//! provider keys among them) can ride into the sidecar or any child it
//! spawns. The model key travels in band, on the init envelope.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use tokio::io::{AsyncBufRead, AsyncWrite};

/// The minimum Node major version the sidecar runs on.
const MIN_NODE_MAJOR: u32 = 22;

/// A boxed stdin half of one sidecar connection.
pub(crate) type BoxWriter = Pin<Box<dyn AsyncWrite + Send>>;
/// A boxed buffered stdout half of one sidecar connection.
pub(crate) type BoxReader = Pin<Box<dyn AsyncBufRead + Send>>;

/// Kill hook so the relay can terminate the sidecar without knowing its
/// process model.
pub(crate) trait AgentKiller: Send {
    /// Begins the kill; a failure here almost always means the child
    /// already exited, which is the state the caller wants anyway.
    fn start_kill(&mut self);
}

impl AgentKiller for tokio::process::Child {
    fn start_kill(&mut self) {
        if let Err(e) = tokio::process::Child::start_kill(self) {
            log::debug!("agent: the sidecar kill failed (likely already exited): {e}");
        }
    }
}

/// One sidecar connection's IO halves plus its kill hook.
pub(crate) struct ProcessIo {
    /// The sidecar's stdin.
    pub(crate) writer: BoxWriter,
    /// The sidecar's buffered stdout.
    pub(crate) reader: BoxReader,
    /// The process kill hook.
    pub(crate) killer: Box<dyn AgentKiller>,
}

/// Factory the relay calls per (re)start.
pub(crate) type SpawnFn = Arc<dyn Fn() -> Result<ProcessIo, String> + Send + Sync>;

/// Locates a usable Node runtime: the `ASTRA_AGENT_NODE` override first,
/// then the PATH search. Every candidate is version gated at 22 or newer;
/// an unusable candidate does not stop the search, because another PATH
/// entry may hold a newer node.
///
/// # Errors
/// Carries a typed prefix (`node-not-found:`) for the UI to render setup
/// guidance from.
pub(crate) fn find_node() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("ASTRA_AGENT_NODE") {
        if !path.is_empty() {
            return check_node(Path::new(&path));
        }
    }
    let candidates: &[&str] = if cfg!(windows) {
        &["node.exe", "node.cmd", "node"]
    } else {
        &["node"]
    };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in candidates {
                let candidate = dir.join(name);
                if candidate.is_file() && check_node(&candidate).is_ok() {
                    return Ok(strip_verbatim_prefix(&candidate));
                }
            }
        }
    }
    Err("node-not-found: searched ASTRA_AGENT_NODE and PATH".to_string())
}

/// Validates one node binary: `--version` must exit successfully and name
/// a major of 22 or newer.
///
/// # Errors
/// Carries the `node-unusable:` or `node-too-old:` prefix.
pub(crate) fn check_node(path: &Path) -> Result<PathBuf, String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("node-unusable: {}: {e}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "node-unusable: {}: --version exited with {}",
            path.display(),
            output.status
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.trim().to_string();
    match parse_node_version(&version) {
        Some((major, _, _)) if major >= MIN_NODE_MAJOR => Ok(strip_verbatim_prefix(path)),
        _ => Err(format!("node-too-old:{version} (need >= {MIN_NODE_MAJOR})")),
    }
}

/// Parses a `node --version` line (`v24.1.0`) into `(major, minor, patch)`.
pub(crate) fn parse_node_version(line: &str) -> Option<(u32, u32, u32)> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let mut parts = body.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Resolves the sidecar entry script:
///   1. the `ASTRA_AGENT_SIDECAR` override (which must exist),
///   2. the dev layout beside the repo (`sidecar/dist/index.cjs`),
///   3. the bundled resource (`<exe_dir>/sidecar/index.cjs`).
///
/// # Errors
/// Carries the `sidecar-script-not-found:` prefix naming what was tried.
pub(crate) fn resolve_sidecar_script() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("ASTRA_AGENT_SIDECAR") {
        if !path.is_empty() {
            let path = PathBuf::from(&path);
            if path.is_file() {
                return Ok(strip_verbatim_prefix(&path));
            }
            return Err(format!(
                "sidecar-script-not-found: ASTRA_AGENT_SIDECAR points at a missing file: {}",
                path.display()
            ));
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sidecar/dist/index.cjs");
    if dev.is_file() {
        return Ok(strip_verbatim_prefix(&dev));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let prod = dir.join("sidecar").join("index.cjs");
            if prod.is_file() {
                return Ok(strip_verbatim_prefix(&prod));
            }
        }
    }
    Err(format!(
        "sidecar-script-not-found: looked beside the repo at {} and beside the executable",
        dev.display()
    ))
}

/// Resolves the astra-cli binary the sidecar's safe-exec runs: the
/// `ASTRA_AGENT_CLI` override, the copy bundled beside the host
/// executable, then the workspace's debug and release builds. The empty
/// string when nothing resolves, which the sidecar turns into a loud
/// per-call failure rather than a broken sidecar.
pub(crate) fn resolve_cli_path() -> String {
    if let Ok(path) = std::env::var("ASTRA_AGENT_CLI") {
        if !path.is_empty() && Path::new(&path).is_file() {
            return path;
        }
    }
    let exe_name = if cfg!(windows) {
        "astra-cli.exe"
    } else {
        "astra-cli"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join(exe_name);
            if beside.is_file() {
                return beside.to_string_lossy().into_owned();
            }
        }
    }
    for profile in ["debug", "release"] {
        let built = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(profile)
            .join(exe_name);
        if built.is_file() {
            return built.to_string_lossy().into_owned();
        }
    }
    String::new()
}

/// The sidecar spawn environment: `PATH`, `ASTRA_DATA_DIR`, and Node's
/// own platform minimums (`SystemRoot` and `SystemDrive` on Windows, whose
/// crypto stack asserts without them), and nothing else. Names match
/// case-insensitively on Windows, where the variables arrive in any case
/// (`SYSTEMROOT` is the common spelling). Taking the parent environment as
/// an iterable keeps this testable without mutating the process
/// environment.
pub(crate) fn spawn_env(
    parent: impl IntoIterator<Item = (String, String)>,
    data_dir: &Path,
) -> BTreeMap<String, String> {
    let allowed = |name: &str| {
        if cfg!(windows) {
            name.eq_ignore_ascii_case("PATH")
                || name.eq_ignore_ascii_case("SystemRoot")
                || name.eq_ignore_ascii_case("SystemDrive")
        } else {
            name == "PATH"
        }
    };
    let mut env = BTreeMap::new();
    for (name, value) in parent {
        if allowed(&name) {
            env.insert(name, value);
        }
    }
    env.insert(
        "ASTRA_DATA_DIR".to_string(),
        data_dir.to_string_lossy().into_owned(),
    );
    env
}

/// Builds the production spawn: one closure that starts the node process
/// over the script with the allowlisted environment, `args` after it, and
/// piped stdio.
pub(crate) fn spawn_sidecar(
    node: PathBuf,
    script: PathBuf,
    data_dir: PathBuf,
    args: Vec<String>,
) -> SpawnFn {
    Arc::new(move || {
        let env = spawn_env(std::env::vars(), &data_dir);
        let mut command = tokio::process::Command::new(&node);
        command
            .arg(&script)
            .args(&args)
            .env_clear()
            .envs(&env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        let mut child = command
            .spawn()
            .map_err(|e| format!("node-unusable: {}: {e}", node.display()))?;
        let stdin = child
            .stdin
            .take()
            .expect("the piped stdin is present right after the spawn");
        let stdout = child
            .stdout
            .take()
            .expect("the piped stdout is present right after the spawn");
        Ok(ProcessIo {
            writer: Box::pin(stdin),
            reader: Box::pin(tokio::io::BufReader::new(stdout)),
            killer: Box::new(child),
        })
    })
}

/// Strips the Windows verbatim path prefix (`\\?\`) by hand: std::fs
/// canonicalisation produces such paths and the process APIs choke on some
/// of them. No-op elsewhere.
pub(crate) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_versions_parse_with_tolerance() {
        assert_eq!(parse_node_version("v24.1.0"), Some((24, 1, 0)));
        assert_eq!(parse_node_version("v22.0.0"), Some((22, 0, 0)));
        assert_eq!(parse_node_version("  v24.1.0\n"), Some((24, 1, 0)));
        assert_eq!(parse_node_version("v24"), Some((24, 0, 0)));
        assert_eq!(parse_node_version(""), None);
        assert_eq!(parse_node_version("not-a-version"), None);
    }

    #[test]
    fn the_spawn_env_carries_only_the_allowlist() {
        let mut parent = vec![
            (
                "ASTRA_LLM_API_KEY_OPENAI".to_string(),
                "sk-secret".to_string(),
            ),
            (
                "ASTRA_LLM_BASE_URL_OPENAI".to_string(),
                "https://e".to_string(),
            ),
            ("ASTRA_LLM_CHAT".to_string(), "openai:g".to_string()),
            ("HOME".to_string(), "/home/ada".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ];
        if cfg!(windows) {
            // The uppercase spelling pins the case-insensitive match: the
            // real environment usually carries SYSTEMROOT.
            parent.push(("SYSTEMROOT".to_string(), r"C:\Windows".to_string()));
        }
        let env = spawn_env(parent, Path::new("/tmp/astra-data"));
        let mut expected = BTreeMap::from([
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("ASTRA_DATA_DIR".to_string(), "/tmp/astra-data".to_string()),
        ]);
        if cfg!(windows) {
            expected.insert("SYSTEMROOT".to_string(), r"C:\Windows".to_string());
        }
        assert_eq!(
            env, expected,
            "no ASTRA_LLM_* variable, key or tier list, rides into the spawn"
        );
    }

    #[test]
    fn verbatim_prefixes_strip() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\C:\Users\ada\node.exe")),
            PathBuf::from(r"C:\Users\ada\node.exe")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new("/usr/local/bin/node")),
            PathBuf::from("/usr/local/bin/node")
        );
    }
}
