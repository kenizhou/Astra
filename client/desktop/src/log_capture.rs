// SPDX-License-Identifier: GPL-3.0-only
//! The shared log capture for the desktop tests: one logger for the whole
//! test binary, because the log crate accepts a single installer and two
//! captures racing would leave the loser's buffer empty forever. A test
//! asserts over its own slice: note the length before the act, read after
//! it, and filter to the lines its own code logs.

use std::sync::{Mutex, OnceLock};

/// The captured log lines, one shared buffer for the whole test binary.
static LINES: OnceLock<Mutex<Vec<(log::Level, String)>>> = OnceLock::new();

/// A capture logger over [`LINES`].
struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if let Some(lines) = LINES.get() {
            lines
                .lock()
                .unwrap()
                .push((record.level(), record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

/// The capture logger, as a static so the log crate's install takes it.
static CAPTURE: Capture = Capture;

/// Installs the capture (the first installer wins; every caller shares it)
/// and returns the shared lines.
pub(crate) fn captured() -> &'static Mutex<Vec<(log::Level, String)>> {
    let lines = LINES.get_or_init(|| Mutex::new(Vec::new()));
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::max());
    lines
}
