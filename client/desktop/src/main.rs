// SPDX-License-Identifier: GPL-3.0-only
//! The `astra-desktop` binary: a shim over the library's [`run`], so tests
//! and the shell share one crate.

fn main() {
    astra_desktop::run();
}
