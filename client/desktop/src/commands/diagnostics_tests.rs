// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the diagnostics commands: the tail's shapes (the last n
//! lines, the short file's whole self, the cap on a runaway ask,
//! nothing when no file stands) and the export's outcomes over the
//! save seam (the file's bytes under its own name, the cancel, the
//! refused write, the failed read), with the export's product event
//! landing only on the save.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::{log_export_of, log_tail_of, TAIL_MAX};
use crate::commands::attachments::{SaveAttachmentOutcome, SaveHost};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The path the fake dialog answers, named so nothing of the log's
/// own path appears inside it.
const CHOSEN: &str = "C:\\saved\\support-log.bin";

/// The seam's fake: answers the dialog from `answer`, records the
/// suggested name, and writes into memory unless told to refuse.
struct FakeSave {
    answer: Option<PathBuf>,
    refuse_write: Option<String>,
    suggested: Mutex<Option<String>>,
    written: Mutex<Vec<(PathBuf, Vec<u8>)>>,
}

impl FakeSave {
    /// A host whose dialog answers the chosen path.
    fn saving_to(path: &str) -> Self {
        Self {
            answer: Some(PathBuf::from(path)),
            refuse_write: None,
            suggested: Mutex::new(None),
            written: Mutex::new(Vec::new()),
        }
    }

    /// A host whose dialog the reader cancels.
    fn cancelled() -> Self {
        Self {
            answer: None,
            refuse_write: None,
            suggested: Mutex::new(None),
            written: Mutex::new(Vec::new()),
        }
    }

    /// A host whose file write refuses with `sentence`.
    fn refusing_write(sentence: &str) -> Self {
        Self {
            answer: Some(PathBuf::from(CHOSEN)),
            refuse_write: Some(sentence.to_owned()),
            suggested: Mutex::new(None),
            written: Mutex::new(Vec::new()),
        }
    }
}

impl SaveHost for FakeSave {
    fn ask_save_path(&self, file_name: &str) -> Option<PathBuf> {
        *self.suggested.lock().unwrap() = Some(file_name.to_owned());
        self.answer.clone()
    }

    fn write(&self, path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
        if let Some(sentence) = &self.refuse_write {
            Err(sentence.clone())
        } else {
            self.written
                .lock()
                .unwrap()
                .push((path.to_owned(), bytes.to_vec()));
            Ok(())
        }
    }
}

/// A state whose log path holds `body`.
fn seeded(name: &str, app: &Arc<FakeApp>, body: &str) -> crate::app_state::AppState {
    let state = state(name, Arc::clone(app), FakeStore::accepting());
    std::fs::write(&state.log_path, body).expect("the log fixture writes");
    state
}

#[test]
fn the_tail_answers_the_last_n_lines() {
    let app = FakeApp::new();
    let state = seeded("tail", &app, "one\ntwo\nthree\nfour\n");

    assert_eq!(
        log_tail_of(&state, 2),
        vec!["three".to_owned(), "four".to_owned()],
        "the ask names how many of the newest lines answer"
    );
    assert_eq!(
        log_tail_of(&state, 10),
        vec![
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
            "four".to_owned()
        ],
        "a file holding fewer than the ask answers all of it"
    );
}

#[test]
fn the_tail_caps_a_runaway_ask() {
    let app = FakeApp::new();
    let mut body = String::new();
    for index in 0..TAIL_MAX + 200 {
        use std::fmt::Write as _;
        let _ = writeln!(body, "line {index}");
    }
    let state = seeded("tail-cap", &app, &body);

    let tail = log_tail_of(&state, TAIL_MAX + 200);
    assert_eq!(
        tail.len(),
        TAIL_MAX,
        "the cap answers the cap's worth, whatever the ask"
    );
    assert_eq!(
        tail.last().map(String::as_str),
        Some(format!("line {}", TAIL_MAX + 199).as_str()),
        "the newest lines are the ones kept"
    );
}

#[test]
fn the_tail_answers_nothing_when_no_file_stands() {
    let app = FakeApp::new();
    let state = state("tail-missing", Arc::clone(&app), FakeStore::accepting());

    assert!(
        log_tail_of(&state, 50).is_empty(),
        "a log file that does not stand answers no lines"
    );
}

#[test]
fn the_export_writes_the_current_file_s_bytes_under_its_own_name() {
    let app = FakeApp::new();
    let body = "1770000000 INFO: boot: the app core is up over 1 account(s)\n";
    let state = seeded("export", &app, body);
    let host = FakeSave::saving_to(CHOSEN);

    let outcome = log_export_of(&state, &host);

    assert_eq!(
        outcome,
        SaveAttachmentOutcome::Saved {
            path: CHOSEN.to_owned(),
        },
        "the saved path is the answer"
    );
    assert_eq!(
        *host.suggested.lock().unwrap(),
        Some("astra.log".to_owned()),
        "the dialog opens on the log's own file name"
    );
    assert_eq!(
        *host.written.lock().unwrap(),
        vec![(PathBuf::from(CHOSEN), body.as_bytes().to_vec())],
        "the current file's whole bytes land at the chosen path"
    );
    let events = app.analytics.lock().unwrap();
    assert!(
        matches!(
            events.as_slice(),
            [event] if event.shape == "log_exported"
                && event.counts.get("bytes").copied() == Some(body.len() as u64)
        ),
        "the export records its own event with the byte count: {events:?}"
    );
}

#[test]
fn the_export_answers_the_cancel_and_the_refused_write() {
    let cancelled_app = FakeApp::new();
    let cancelled_state = seeded("export-cancel", &cancelled_app, "a line\n");
    let host = FakeSave::cancelled();
    let outcome = log_export_of(&cancelled_state, &host);
    assert_eq!(outcome, SaveAttachmentOutcome::Cancelled);
    assert!(
        host.written.lock().unwrap().is_empty(),
        "a cancelled dialog writes nothing"
    );
    assert!(
        cancelled_app.analytics.lock().unwrap().is_empty(),
        "a cancelled export records no event"
    );

    let refused_app = FakeApp::new();
    let refused_state = seeded("export-refused", &refused_app, "a line\n");
    let host = FakeSave::refusing_write("the disk is full");
    let outcome = log_export_of(&refused_state, &host);
    assert_eq!(
        outcome,
        SaveAttachmentOutcome::Failed {
            reason: "the disk is full".to_owned(),
        },
        "the write's own sentence is the reason"
    );
    assert!(
        refused_app.analytics.lock().unwrap().is_empty(),
        "a refused write records no event"
    );
}

#[test]
fn a_missing_log_file_answers_failed() {
    let app = FakeApp::new();
    let state = state("export-missing", Arc::clone(&app), FakeStore::accepting());
    let host = FakeSave::saving_to(CHOSEN);

    let outcome = log_export_of(&state, &host);

    let SaveAttachmentOutcome::Failed { reason } = outcome else {
        panic!("the missing file answers failed");
    };
    assert!(
        reason.contains("os error"),
        "the read's own sentence is the reason: {reason}"
    );
    assert!(
        host.suggested.lock().unwrap().is_none(),
        "a failed read never opens the dialog"
    );
    assert!(
        host.written.lock().unwrap().is_empty(),
        "a failed read writes nothing"
    );
    assert!(
        app.analytics.lock().unwrap().is_empty(),
        "a failed export records no event"
    );
}
