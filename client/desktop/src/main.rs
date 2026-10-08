// SPDX-License-Identifier: GPL-3.0-only
//! The `astra-desktop` binary: a shim over the library's [`run`], so tests
//! and the shell share one crate. The one argument it understands is the
//! headless slice flag, which runs the same host without the window; any
//! other argument is refused rather than half-ignored.

fn main() {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("--headless-slice"), Some(phase)) => {
            std::process::exit(astra_desktop::run_headless_slice(phase));
        }
        (Some("--headless-slice"), None) => {
            eprintln!("astra-desktop: --headless-slice needs a phase (add or list)");
            std::process::exit(2);
        }
        (Some(other), _) => {
            eprintln!("astra-desktop: unknown argument '{other}'");
            std::process::exit(2);
        }
        (None, _) => astra_desktop::run(),
    }
}
