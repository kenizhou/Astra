// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the key import commands: the webview's account and bytes hand
//! through to the core, the core's row is the answer, the list answers the
//! account's rows, and the passphrase never reaches the log. The fake app
//! and the state builder live in `commands_fakes`, one compiled copy for
//! every command test module.

use std::sync::Arc;

use super::{import_key_of, list_keys_of};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The account the import names: the same id the reading fixtures use.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").expect("an account id")
}

/// A public-view row the fake answers with.
fn row(fingerprint: &str) -> astra_crypto_store::CryptoKeyRow {
    astra_crypto_store::CryptoKeyRow {
        id: "row-1".to_owned(),
        account_id: "work".to_owned(),
        standard: "openpgp".to_owned(),
        key_type: "public".to_owned(),
        email: Some("peer@elsewhere.local".to_owned()),
        fingerprint: fingerprint.to_owned(),
        origin: "imported".to_owned(),
        created_at: "1700000000".to_owned(),
        ..Default::default()
    }
}

#[test]
fn the_import_hands_the_account_and_bytes_through_and_answers_the_row() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().import = Some(Ok(row("fpImport")));
    let state = state("keys-import", Arc::clone(&app), FakeStore::accepting());

    let outcome = import_key_of(&state, &account(), b"the key bytes".to_vec(), None);

    assert_eq!(outcome.expect("the row answers").fingerprint, "fpImport");
    assert_eq!(
        app.keys.lock().unwrap().import_into.as_slice(),
        [("work".to_owned(), 13)],
        "the command hands the account and the byte count through to the core"
    );
}

#[test]
fn a_refusal_answers_the_cores_words() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().import =
        Some(Err("the key import failed: wrong passphrase".to_owned()));
    let state = state("keys-refuse", Arc::clone(&app), FakeStore::accepting());

    let outcome = import_key_of(&state, &account(), b"bytes".to_vec(), None);

    assert_eq!(
        outcome.expect_err("the refusal crosses"),
        "the key import failed: wrong passphrase"
    );
}

#[test]
fn the_list_answers_the_accounts_rows() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().list = Some(Ok(vec![row("fpA"), row("fpB")]));
    let state = state("keys-list", Arc::clone(&app), FakeStore::accepting());

    let outcome = list_keys_of(&state, &account()).expect("the list answers");

    assert_eq!(outcome.len(), 2);
    assert_eq!(outcome[0].fingerprint, "fpA");
    assert_eq!(
        app.keys.lock().unwrap().list_into.as_slice(),
        ["work".to_owned()],
        "the command hands the webview's account through to the core"
    );
}

#[test]
fn the_passphrase_never_reaches_the_log() {
    let lines = crate::log_capture::captured();
    let before = lines.lock().unwrap().len();
    // A passphrase unique to this test: the other command tests log their own
    // lines concurrently into the same shared buffer.
    let probe = "key-import-passphrase-probe-5c8d2";
    let app = FakeApp::new();
    app.keys.lock().unwrap().import = Some(Ok(row("fpSecret")));
    let state = state("keys-log", Arc::clone(&app), FakeStore::accepting());

    let outcome = import_key_of(
        &state,
        &account(),
        b"the secret key bytes".to_vec(),
        Some(probe.to_owned()),
    );
    assert!(outcome.is_ok());

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
