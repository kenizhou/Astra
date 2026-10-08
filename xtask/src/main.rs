// SPDX-License-Identifier: GPL-3.0-only
//! Astra task runner. `gate` runs every contract check; `check <name>` runs one.
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod checks;
mod icons;

fn main() -> ExitCode {
    let root = repo_root();
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("gate"), None) => gate(&root),
        (Some("check"), Some(name)) => one(&root, name),
        (Some("icons"), None) => {
            let written = icons::generate(
                &root.join("branding/icon-source.jpg"),
                &root.join("client/desktop/icons"),
            );
            match written {
                Ok(files) => {
                    for f in files {
                        println!("{}", f.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("icons: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: cargo run -p xtask -- gate | check <name> | icons");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|d| d.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn gate(root: &Path) -> ExitCode {
    let mut violations = 0usize;
    for check in checks::all() {
        print!("{} ... ", check.name);
        match (check.run)(root) {
            Ok(()) => println!("PASS"),
            Err(msgs) => {
                violations += msgs.len();
                println!("FAIL");
                for m in msgs {
                    println!("  {m}");
                }
            }
        }
    }
    if violations == 0 {
        println!("gate: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("gate: {violations} violation(s)");
        ExitCode::FAILURE
    }
}

fn one(root: &Path, name: &str) -> ExitCode {
    let Some(check) = checks::all().into_iter().find(|c| c.name == name) else {
        eprintln!("unknown check: {name}");
        return ExitCode::FAILURE;
    };
    match (check.run)(root) {
        Ok(()) => {
            println!("{name}: PASS");
            ExitCode::SUCCESS
        }
        Err(msgs) => {
            for m in msgs {
                eprintln!("{m}");
            }
            ExitCode::FAILURE
        }
    }
}
