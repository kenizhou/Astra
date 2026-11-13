// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the password re-entry command: the webview's ids hand through to
//! the core, the core's prompt affordance is the answer (the re-prompt signal
//! and the decrypt's null), and the passphrase never reaches the log. The
//! fake app and the state builder live in `commands_fakes`, one compiled copy
//! for every command test module.

use std::sync::Arc;

use astra_viewmodel::PasswordPromptState;

use super::submit_reading_password_of;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The account and provider key the submission names: the same pair the
/// reading fixtures use.
fn key_parts() -> (engine_api::AccountId, engine_api::ProviderKey) {
    (
        engine_api::AccountId::try_from("work").expect("an account id"),
        engine_api::ProviderKey::new("imap:v1:u42").expect("a provider key"),
    )
}

#[test]
fn a_wrong_passphrase_answers_the_re_prompt_signal_and_hands_the_ids_through() {
    let app = FakeApp::new();
    *app.password_prompt.lock().unwrap() = Some(PasswordPromptState::WrongPassword);
    let state = state("crypto-wrong", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    let outcome =
        submit_reading_password_of(&state, &account, &key, "not-the-passphrase".to_owned());

    assert_eq!(outcome, Some(PasswordPromptState::WrongPassword));
    assert_eq!(
        app.password_into.lock().unwrap().as_slice(),
        [("work".to_owned(), "imap:v1:u42".to_owned())],
        "the command hands the webview's ids through to the core"
    );
}

#[test]
fn a_decrypt_answers_null_once_the_affordance_drops() {
    let app = FakeApp::new();
    *app.password_prompt.lock().unwrap() = None;
    let state = state("crypto-ok", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    let outcome = submit_reading_password_of(&state, &account, &key, "hunter2".to_owned());

    assert_eq!(outcome, None, "the decrypt drops the prompt affordance");
}

#[test]
fn the_passphrase_never_reaches_the_log() {
    let lines = crate::log_capture::captured();
    let before = lines.lock().unwrap().len();
    // A passphrase unique to this test: the other command tests log their own
    // lines concurrently into the same shared buffer.
    let probe = "passphrase-log-probe-7f3a9c";
    let app = FakeApp::new();
    *app.password_prompt.lock().unwrap() = Some(PasswordPromptState::WrongPassword);
    let state = state("crypto-log", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    let outcome = submit_reading_password_of(&state, &account, &key, probe.to_owned());
    assert_eq!(outcome, Some(PasswordPromptState::WrongPassword));

    let mine = lines
        .lock()
        .unwrap()
        .iter()
        .skip(before)
        .map(|(_, message)| message.clone())
        .collect::<Vec<_>>();
    for line in &mine {
        assert!(
            !line.contains(probe),
            "the passphrase is never logged: {line}"
        );
    }
}
