// SPDX-License-Identifier: GPL-3.0-only
//! The `astra-cli` binary: the agent-first tool-call entry point (rulebook,
//! CLI rules). A thin shim over [`verbs::run`], exactly as the engine's own
//! CLI is shaped: parsing, dispatch, and output live in the modules and are
//! tested there. The envelope prints one line when `--json` asks for it or
//! stdout is piped, and indented when a human is reading a terminal.

use std::io::IsTerminal;
use std::path::PathBuf;

mod audit;
mod envelope;
#[cfg(test)]
mod envelope_tests;
mod verbs;

/// The app identifier whose per-user data directory the CLI defaults to,
/// beside the desktop host's own store (tauri.conf.json spells the same id).
const APP_ID: &str = "com.kenizhou.astra";

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data_dir = match data_dir() {
        Ok(dir) => dir,
        Err(message) => {
            let envelope = envelope::Envelope::failure(envelope::Code::Runtime, message);
            print(&envelope, false);
            return std::process::ExitCode::from(envelope::Code::Runtime.exit());
        }
    };
    let stdin = std::io::stdin();
    let run = verbs::run(&args, &data_dir, &mut stdin.lock());
    print(&run.envelope, !run.json && std::io::stdout().is_terminal());
    std::process::ExitCode::from(run.code.exit())
}

/// Where the stores live: `ASTRA_DATA_DIR` when set, else the platform's
/// per-user data directory under the app identifier.
fn data_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("ASTRA_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    dirs::data_dir()
        .map(|base| base.join(APP_ID))
        .ok_or_else(|| {
            "the platform data directory could not be resolved; set ASTRA_DATA_DIR".to_owned()
        })
}

/// Prints one envelope, compact or indented, falling back to stderr when the
/// text cannot be rendered (which the constructors cannot produce).
fn print(envelope: &envelope::Envelope, pretty: bool) {
    match envelope.render(pretty) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("the envelope could not be rendered: {err}"),
    }
}
