// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the rotating file logger: the rotation shifting capped
//! files (the oldest going, never more standing than the cap names),
//! the level the debug preference names, and the log contract held
//! over a scripted burst (every line the logger writes carries the
//! stamp and level shape, and no secret, mail content, or repository
//! path crosses into any file that stands).

use std::path::PathBuf;

use log::Log as _;

use super::{level_for, FileLogger, LOG_FILE};

/// A scratch directory of this test's own, wiped so one test's files
/// never reach another's.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-logging-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// One record logged through the logger, shaped the way the log crate
/// hands the host's callsites over.
fn log_line(logger: &FileLogger, level: log::Level, message: &str) {
    logger.log(
        &log::Record::builder()
            .level(level)
            .target("astra_desktop::logging_tests")
            .args(format_args!("{message}"))
            .build(),
    );
}

/// Every line the standing files hold, current first, oldest last.
fn every_line(dir: &std::path::Path) -> String {
    let mut every = String::new();
    every.push_str(&std::fs::read_to_string(dir.join(LOG_FILE)).expect("the current file stands"));
    for index in 1..=4 {
        if let Ok(body) = std::fs::read_to_string(dir.join(format!("{LOG_FILE}.{index}"))) {
            every.push_str(&body);
        }
    }
    every
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

#[test]
fn a_burst_rotates_the_files_and_holds_the_cap() {
    let dir = scratch("rotate");
    let logger = FileLogger::with_caps(&dir, 2, 200);

    // A burst past the cap: each line around 60 bytes, so the 200 byte
    // file fills three times over.
    for index in 0..12 {
        log_line(
            &logger,
            log::Level::Info,
            &format!("burst line {index} with some padding to fill the file"),
        );
    }

    // The current file and two rotated stand; the third never does,
    // because the cap is two rotated files.
    assert!(dir.join(LOG_FILE).is_file(), "the current file stands");
    assert!(
        dir.join(format!("{LOG_FILE}.1")).is_file(),
        "the first rotated file stands"
    );
    assert!(
        dir.join(format!("{LOG_FILE}.2")).is_file(),
        "the second rotated file stands"
    );
    assert!(
        !dir.join(format!("{LOG_FILE}.3")).exists(),
        "the cap holds: no third rotated file"
    );

    // Every standing file holds at most the cap.
    for name in [
        LOG_FILE.to_owned(),
        format!("{LOG_FILE}.1"),
        format!("{LOG_FILE}.2"),
    ] {
        let held = std::fs::metadata(dir.join(&name))
            .expect("the file stands")
            .len();
        assert!(
            held <= 200,
            "no file grows past the cap: {name} holds {held}"
        );
    }

    // The burst's newest line stands and its oldest rotated away: the
    // files hold a window, not a history.
    let every = every_line(&dir);
    assert!(every.contains("burst line 11"), "the newest line stands");
    assert!(
        !every.contains("burst line 0"),
        "the oldest line rotated away"
    );
}

#[test]
fn the_written_lines_hold_the_log_contract() {
    let dir = scratch("contract");
    let logger = FileLogger::with_caps(&dir, 2, 180);

    // The scripted burst: lines in the shapes the host's real
    // callsites log (counts, ids, error sentences), driven past a
    // rotation so the contract is greped over every file that stands,
    // the logger's own rotation notice included.
    log_line(
        &logger,
        log::Level::Info,
        "boot: the app core is up over 2 account(s)",
    );
    log_line(
        &logger,
        log::Level::Warn,
        "dispatch: an intent failed: the intent family is not implemented",
    );
    log_line(
        &logger,
        log::Level::Info,
        "agent: the sidecar is ready with 3 model(s)",
    );
    log_line(
        &logger,
        log::Level::Warn,
        "delivery: a watch could not be opened; reconnecting: the socket dropped",
    );
    for index in 0..12 {
        log_line(
            &logger,
            log::Level::Info,
            &format!("sync: pass {index} landed 4 message(s)"),
        );
    }

    let every = every_line(&dir);
    assert!(
        every.contains("the log rotated"),
        "the rotation notice lands in the fresh file"
    );

    // The shape: every line carries the stamp and level word.
    for line in every.lines() {
        assert!(
            shaped(line),
            "the line carries the stamp and level shape: {line}"
        );
    }

    // The forbidden classes: no secrets, no mail content, no
    // repository paths, in any file the logger wrote.
    let lower = every.to_lowercase();
    for word in ["password", "secret", "token", "bearer"] {
        assert!(!lower.contains(word), "no secret marker crosses: {word}");
    }
    assert!(!every.contains('@'), "no mail address crosses");
    for shape in ["D:", "\\\\", "/home/", "src/", "client/", "vendor/"] {
        assert!(
            !every.contains(shape),
            "no repository path crosses: {shape}"
        );
    }
}

#[test]
fn the_level_follows_the_debug_preference() {
    assert_eq!(
        level_for(false),
        log::LevelFilter::Info,
        "the default level is info"
    );
    assert_eq!(
        level_for(true),
        log::LevelFilter::Debug,
        "the Diagnostics toggle raises the level to debug"
    );
}
