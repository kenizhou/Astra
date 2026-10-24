// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the `.eml` command: the save's three outcomes over the seam
//! (the saved path answered and the engine's source bytes written under the
//! subject's suggested name, the reader's cancel, the failed read carrying
//! the engine's own sentence), and the log contract (nothing about the read
//! is logged beyond the saved path the reader chose). The byte honesty is
//! pinned here at the seam (the written bytes are the core's answer,
//! verbatim) and in the app core against the engine itself. The fake app
//! and the state builder live in `commands_fakes`, one compiled copy for
//! every command test module.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use astra_app::{EmlContent, EmlReadError};

use super::eml_save_of;
use crate::commands::attachments::{SaveAttachmentOutcome, SaveHost};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The account and provider key the save names: the same pair the reading
/// fixtures use.
fn key_parts() -> (engine_api::AccountId, engine_api::ProviderKey) {
    (
        engine_api::AccountId::try_from("work").expect("an account id"),
        engine_api::ProviderKey::new("imap:v1:u42").expect("a provider key"),
    )
}

/// The `.eml` content the fake core answers with: the subject's sanitised
/// name beside a source whose folded header and quoted-printable body a
/// re-serialization would tidy away, so the written bytes are recognisable
/// as the engine's own.
fn content() -> EmlContent {
    EmlContent {
        file_name: "Re quarterly report.eml".to_owned(),
        bytes: b"Subject: Re:\r\n quarterly report\r\n\
                 Content-Transfer-Encoding: quoted-printable\r\n\
                 Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
                 caf=C3=A9\r\n"
            .to_vec(),
    }
}

/// The path the fake dialog answers: named so no sentinel of the read (the
/// account, the key, the suggested file name) appears inside it.
const CHOSEN: &str = "C:\\saved\\elsewhere.eml";

/// The seam's fake: answers the dialog from `answer`, records the suggested
/// name, and writes into memory unless told to refuse.
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

/// A state whose fake core answers the fixture `.eml` content.
fn seeded(name: &str, app: &Arc<FakeApp>) -> crate::app_state::AppState {
    *app.eml.lock().unwrap() = Ok(content());
    state(name, Arc::clone(app), FakeStore::accepting())
}

#[test]
fn a_saved_source_answers_the_chosen_path_and_writes_the_engines_bytes() {
    let app = FakeApp::new();
    let state = seeded("eml-save", &app);
    let (account, key) = key_parts();
    let host = FakeSave::saving_to(CHOSEN);

    let outcome = state
        .runtime
        .block_on(|| eml_save_of(&state, &account, &key, &host));

    assert_eq!(
        outcome,
        SaveAttachmentOutcome::Saved {
            path: CHOSEN.to_owned(),
        },
        "the saved path is the answer"
    );
    assert_eq!(
        *host.suggested.lock().unwrap(),
        Some("Re quarterly report.eml".to_owned()),
        "the dialog opens on the subject's own sanitised name"
    );
    assert_eq!(
        *host.written.lock().unwrap(),
        vec![(PathBuf::from(CHOSEN), content().bytes)],
        "the engine's source bytes land at the chosen path, verbatim"
    );
    assert_eq!(
        app.eml_into.lock().unwrap().as_slice(),
        [("work".to_owned(), "imap:v1:u42".to_owned())],
        "the command hands the webview's ids through to the core"
    );
}

#[test]
fn a_cancelled_dialog_answers_cancelled_and_writes_nothing() {
    let app = FakeApp::new();
    let state = seeded("eml-cancel", &app);
    let (account, key) = key_parts();
    let host = FakeSave::cancelled();

    let outcome = state
        .runtime
        .block_on(|| eml_save_of(&state, &account, &key, &host));

    assert_eq!(outcome, SaveAttachmentOutcome::Cancelled);
    assert!(
        host.written.lock().unwrap().is_empty(),
        "a cancelled dialog writes nothing"
    );
}

#[test]
fn a_failed_read_answers_failed_with_the_engines_sentence_and_never_asks() {
    let app = FakeApp::new();
    *app.eml.lock().unwrap() = Err(EmlReadError::Fetch(
        "sync error: provider error: the fake holds no source for that key".to_owned(),
    ));
    let state = state("eml-refused", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();
    let host = FakeSave::saving_to(CHOSEN);

    let outcome = state
        .runtime
        .block_on(|| eml_save_of(&state, &account, &key, &host));

    let SaveAttachmentOutcome::Failed { reason } = outcome else {
        panic!("the refused read answers failed: {outcome:?}");
    };
    assert!(
        reason.contains("the fake holds no source for that key"),
        "the engine's own sentence is the reason: {reason}"
    );
    assert!(
        host.suggested.lock().unwrap().is_none(),
        "a failed read never opens the dialog"
    );
    assert!(
        host.written.lock().unwrap().is_empty(),
        "a failed read writes nothing"
    );
}

#[test]
fn a_refused_write_answers_failed_with_the_write_sentence() {
    let app = FakeApp::new();
    let state = seeded("eml-write-refused", &app);
    let (account, key) = key_parts();
    let host = FakeSave::refusing_write("the disk is full");

    let outcome = state
        .runtime
        .block_on(|| eml_save_of(&state, &account, &key, &host));

    assert_eq!(
        outcome,
        SaveAttachmentOutcome::Failed {
            reason: "the disk is full".to_owned(),
        },
        "the write's own sentence is the reason"
    );
}

#[test]
fn nothing_is_logged_beyond_the_saved_path_the_user_chose() {
    let lines = crate::log_capture::captured();
    let before = lines.lock().unwrap().len();
    // The probe's own chosen path, unique to this test: the other command
    // tests log their own saves concurrently into the same shared buffer.
    let probe = "C:\\saved\\eml-log-probe.bin";

    // Drive all three outcomes: the saved path is logged, the cancel and
    // the refusal carry only their sentences.
    let saved_app = FakeApp::new();
    let saved_state = seeded("eml-log-saved", &saved_app);
    let (account, key) = key_parts();
    let host = FakeSave::saving_to(probe);
    let saved = saved_state
        .runtime
        .block_on(|| eml_save_of(&saved_state, &account, &key, &host));
    assert!(matches!(saved, SaveAttachmentOutcome::Saved { .. }));

    let cancelled_app = FakeApp::new();
    let cancelled_state = seeded("eml-log-cancel", &cancelled_app);
    let host = FakeSave::cancelled();
    let cancelled = cancelled_state
        .runtime
        .block_on(|| eml_save_of(&cancelled_state, &account, &key, &host));
    assert_eq!(cancelled, SaveAttachmentOutcome::Cancelled);

    let failed_app = FakeApp::new();
    *failed_app.eml.lock().unwrap() = Err(EmlReadError::Unresolved);
    let failed_state = state(
        "eml-log-failed",
        Arc::clone(&failed_app),
        FakeStore::accepting(),
    );
    let host = FakeSave::saving_to(probe);
    let failed = failed_state
        .runtime
        .block_on(|| eml_save_of(&failed_state, &account, &key, &host));
    assert!(matches!(failed, SaveAttachmentOutcome::Failed { .. }));

    let mine = lines
        .lock()
        .unwrap()
        .iter()
        .skip(before)
        .map(|(_, message)| message.clone())
        .filter(|message| message.contains("save-eml"))
        .collect::<Vec<_>>();
    assert!(!mine.is_empty(), "the save's own lines are captured");
    for line in &mine {
        assert!(!line.contains("work"), "no account id: {line}");
        assert!(!line.contains("imap:v1:u42"), "no message key: {line}");
        assert!(!line.contains("quarterly"), "no suggested name: {line}");
    }
    assert_eq!(
        mine.iter().filter(|line| line.contains(probe)).count(),
        1,
        "the saved path is named exactly once, by the save that chose it: {mine:?}"
    );
}
