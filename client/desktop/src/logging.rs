// SPDX-License-Identifier: GPL-3.0-only
//! The windowed run's production logger: the headless StderrLogger's
//! sibling, writing the app data dir instead of stderr, capped in
//! files and bytes. Five files stand at the most (the current
//! `astra.log` beside `astra.log.1` through `astra.log.4`), each
//! holding at most one mebibyte before the rotation shifts, so a
//! chatty run costs the disk five mebibytes and never more.
//!
//! The log contract holds from the first line: no secrets, no mail
//! content, no repository paths. The logger's own lines (the install
//! notice, the rotation notice) carry neither, and the host's
//! callsites follow the discipline the codebase already holds. The
//! level is read at boot from the persisted debug preference (info
//! unless the Diagnostics toggle stands, debug when it does), so the
//! change lands with the next launch, the honest wording the settings
//! surface states.
//!
//! A file that cannot open leaves the run unlogged, the state before
//! this logger stood, with the reason on stderr: a diagnostic that
//! cannot start never fails the app over it.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// The log file's name, under the app data directory beside the
/// stores: the same naming style the store files follow.
pub(crate) const LOG_FILE: &str = "astra.log";

/// How many rotated files stand beside the current one (`astra.log.1`
/// through `astra.log.4`): five files all told, the small honest
/// figure the Diagnostics row names.
const ROTATED_FILES: usize = 4;

/// The byte cap one file holds before the rotation shifts: one
/// mebibyte.
const MAX_BYTES: u64 = 1024 * 1024;

/// The current log file's path under `data_dir`: the path the tail and
/// export commands read and the logger writes.
pub(crate) fn file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LOG_FILE)
}

/// The level the boot log runs at: debug when the Diagnostics toggle
/// stands, info otherwise.
pub(crate) fn level_for(debug_enabled: bool) -> log::LevelFilter {
    if debug_enabled {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    }
}

/// The logger's open half: the current file and the bytes it already
/// holds, so the rotation decision needs no second stat.
struct LogFile {
    file: std::fs::File,
    size: u64,
}

/// The rotating file logger: one open current file, the rotation's
/// caps, and the directory both live in.
pub(crate) struct FileLogger {
    dir: PathBuf,
    rotated: usize,
    max_bytes: u64,
    current: Mutex<Option<LogFile>>,
}

impl FileLogger {
    /// The production logger over `dir`: four rotated files of one
    /// mebibyte each beside the current one.
    pub(crate) fn new(dir: &Path) -> Self {
        Self::with_caps(dir, ROTATED_FILES, MAX_BYTES)
    }

    /// A logger with the caps named, the seam the tests' small figures
    /// drive. Opens the current file; a refusal leaves the logger
    /// dark, which the caller states its own way.
    fn with_caps(dir: &Path, rotated: usize, max_bytes: u64) -> Self {
        let logger = Self {
            dir: dir.to_owned(),
            rotated,
            max_bytes,
            current: Mutex::new(None),
        };
        logger.open();
        logger
    }

    /// Opens (or appends to) the current file, creating the directory
    /// when the boot has not yet made it.
    fn open(&self) -> bool {
        let _ = std::fs::create_dir_all(&self.dir);
        let opened = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(LOG_FILE));
        match opened {
            Ok(file) => {
                let size = file.metadata().map_or(0, |meta| meta.len());
                *self.current.lock().expect("the log mutex is not poisoned") =
                    Some(LogFile { file, size });
                true
            }
            Err(_) => false,
        }
    }

    /// Rotates under the caller's lock: the handle closes first
    /// (Windows refuses to rename an open file), the oldest rotated
    /// file goes, the rest shift one number up, and the current file
    /// becomes `.1` while a fresh current opens carrying the rotation
    /// notice as its first line. A refusal anywhere leaves the logger
    /// dark rather than failing the app over a diagnostic.
    fn rotate(&self, current: &mut Option<LogFile>) {
        *current = None;
        let _ = std::fs::remove_file(self.dir.join(format!("{LOG_FILE}.{}", self.rotated)));
        for index in (1..self.rotated).rev() {
            let _ = std::fs::rename(
                self.dir.join(format!("{LOG_FILE}.{index}")),
                self.dir.join(format!("{LOG_FILE}.{}", index + 1)),
            );
        }
        if std::fs::rename(
            self.dir.join(LOG_FILE),
            self.dir.join(format!("{LOG_FILE}.1")),
        )
        .is_err()
        {
            return;
        }
        let fresh = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(LOG_FILE));
        let Ok(file) = fresh else {
            return;
        };
        let mut open = LogFile { file, size: 0 };
        let notice = line(log::Level::Info, "logging: the log rotated");
        if open.file.write_all(notice.as_bytes()).is_ok() {
            open.size += notice.len() as u64;
        }
        *current = Some(open);
    }
}

/// One line as the file holds it: the epoch stamp, the level word,
/// then the message. The stamp is the std clock's unix seconds, no
/// dependency pulled for a diagnostic.
fn line(level: log::Level, message: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    format!("{stamp} {level}: {message}\n")
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let body = line(record.level(), &record.args().to_string());
        let mut current = self.current.lock().expect("the log mutex is not poisoned");
        let size = current.as_ref().map_or(0, |open| open.size);
        // The rotation fires only when the current file holds
        // something: a single line longer than the cap stands alone in
        // its own file (a line is never split), and an empty file
        // never rotates.
        if size > 0 && size + body.len() as u64 > self.max_bytes {
            self.rotate(&mut current);
        }
        let Some(open) = current.as_mut() else {
            return;
        };
        if open.file.write_all(body.as_bytes()).is_ok() {
            open.size += body.len() as u64;
        }
    }

    fn flush(&self) {}
}

/// Installs the file logger over `data_dir`, the level the persisted
/// debug preference names. The preferences file is read directly (the
/// core's own copy stands only after the assembly, which itself logs),
/// so the level is the boot's first fact. A file that cannot open
/// leaves the run unlogged, and a logger already installed stays the
/// one the run keeps.
pub(crate) fn install(data_dir: &Path) {
    let logger = FileLogger::new(data_dir);
    let opened = logger
        .current
        .lock()
        .expect("the log mutex is not poisoned")
        .is_some();
    if !opened {
        eprintln!("logging: the log file could not open; the run stays unlogged");
        return;
    }
    let debug = astra_app::load_preferences(astra_app::preferences_path(data_dir)).debug_enabled;
    let level = level_for(debug);
    // The logger is installed for the process's life: the box is
    // leaked into the log crate's static slot, the one installer a
    // process gets.
    let logger: &'static FileLogger = Box::leak(Box::new(logger));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
        log::info!("logging: the file log stands at {level}");
    }
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
