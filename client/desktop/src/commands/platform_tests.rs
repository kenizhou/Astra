// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the default-mail registration verb and its offer latch: the
//! idempotence (the first ask registers, the second finds it standing and
//! writes nothing), the refused registration's typed outcome, the landed
//! registration's product event, the outcome's wire shape, and the
//! ask-once latch file the asking surface reads and marks.

use std::sync::{Arc, Mutex};

use super::{
    default_mail_asked_at, mark_default_mail_asked_at, register_default_mail_with, DefaultMailHost,
    RegisterDefaultMailOutcome,
};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The seam's fake: a standing flag the registration raises, with the
/// register call count, scriptable to refuse.
struct FakeMail {
    refuses: Option<String>,
    standing: Mutex<bool>,
    registers: Mutex<usize>,
}

impl FakeMail {
    /// A host whose registration has not been made.
    fn cold() -> Self {
        Self {
            refuses: None,
            standing: Mutex::new(false),
            registers: Mutex::new(0),
        }
    }

    /// A host whose every ask refuses with `sentence`.
    fn refusing(sentence: &str) -> Self {
        Self {
            refuses: Some(sentence.to_owned()),
            standing: Mutex::new(false),
            registers: Mutex::new(0),
        }
    }
}

impl DefaultMailHost for FakeMail {
    fn is_registered(&self) -> Result<bool, String> {
        if let Some(sentence) = &self.refuses {
            return Err(sentence.clone());
        }
        Ok(*self.standing.lock().unwrap())
    }

    fn register(&self) -> Result<(), String> {
        if let Some(sentence) = &self.refuses {
            return Err(sentence.clone());
        }
        *self.standing.lock().unwrap() = true;
        *self.registers.lock().unwrap() += 1;
        Ok(())
    }
}

#[test]
fn the_registration_is_idempotent() {
    let app = FakeApp::new();
    let state = state("mail-idempotent", Arc::clone(&app), FakeStore::accepting());
    let host = FakeMail::cold();

    assert_eq!(
        register_default_mail_with(&state, &host),
        RegisterDefaultMailOutcome::Registered,
        "the first ask over no standing registration registers"
    );
    assert_eq!(
        register_default_mail_with(&state, &host),
        RegisterDefaultMailOutcome::AlreadyDefault,
        "the second ask finds the registration standing"
    );
    assert_eq!(
        *host.registers.lock().unwrap(),
        1,
        "a standing registration is never re-asserted: the OS is written once"
    );
}

#[test]
fn the_landed_registration_records_the_product_event() {
    let app = FakeApp::new();
    let state = state("mail-event", Arc::clone(&app), FakeStore::accepting());
    let host = FakeMail::cold();

    let outcome = register_default_mail_with(&state, &host);
    assert_eq!(outcome, RegisterDefaultMailOutcome::Registered);

    let events = app.analytics.lock().unwrap();
    assert!(
        matches!(events.as_slice(), [event] if event.shape == "default_mail_registered"),
        "the landed registration records its own event: {events:?}"
    );
}

#[test]
fn a_refused_registration_answers_failed_and_records_nothing() {
    let app = FakeApp::new();
    let state = state("mail-refused", Arc::clone(&app), FakeStore::accepting());
    let host = FakeMail::refusing("the platform refused the write");

    assert_eq!(
        register_default_mail_with(&state, &host),
        RegisterDefaultMailOutcome::Failed {
            reason: "the platform refused the write".to_owned(),
        },
        "the refusal's own sentence is the reason"
    );
    assert!(
        app.analytics.lock().unwrap().is_empty(),
        "a refused registration records no event"
    );
}

#[test]
fn the_outcome_crosses_the_wire_as_the_snake_case_tag() {
    let wire =
        serde_json::to_value(RegisterDefaultMailOutcome::Registered).expect("the tag serialises");
    assert_eq!(wire, serde_json::json!("registered"));

    let wire = serde_json::to_value(RegisterDefaultMailOutcome::AlreadyDefault)
        .expect("the standing tag serialises");
    assert_eq!(wire, serde_json::json!("already_default"));

    let wire = serde_json::to_value(RegisterDefaultMailOutcome::Failed {
        reason: "the platform refused the write".to_owned(),
    })
    .expect("the failure serialises");
    assert_eq!(
        wire,
        serde_json::json!({"failed": {"reason": "the platform refused the write"}})
    );
}

/// One scratch directory under the target dir, named per test.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-mail-latch-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_latch_reads_unset_until_the_ask_marks_it() {
    let dir = scratch("unset-then-marked");

    assert!(
        !default_mail_asked_at(&dir),
        "an untouched data dir has never been asked"
    );

    mark_default_mail_asked_at(&dir).expect("the mark writes");
    assert!(
        default_mail_asked_at(&dir),
        "the marked data dir answers asked"
    );
}

#[test]
fn the_mark_is_idempotent_and_recreates_its_directory() {
    let dir = scratch("idempotent-mark").join("nested");

    mark_default_mail_asked_at(&dir).expect("the first mark makes its directory");
    mark_default_mail_asked_at(&dir).expect("the second mark is idempotent");
    assert!(default_mail_asked_at(&dir));
}
