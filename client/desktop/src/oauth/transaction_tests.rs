// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the credential transaction: the three answer paths against
//! a scripted store whose persist and delete can each be told to refuse
//! independently (a real platform store refuses them for different
//! reasons), and the synthetic grant round trip that proves the paths in
//! sequence. The rotation's failure report is captured through the shared
//! installed logger, because "logged at error" is the contract, not a
//! return value.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use super::CredentialTransaction;
use crate::credential_store::{AccountCredentialStore, CredentialStoreError};
use crate::log_capture::captured;

/// A scripted store: every landed write and erase in order, with the
/// refusal each half can be told to answer.
struct ScriptedStore {
    persisted: Mutex<Vec<(String, String)>>,
    deleted: Mutex<Vec<String>>,
    refuse_persist: Mutex<Option<String>>,
    refuse_delete: Mutex<Option<String>>,
}

impl ScriptedStore {
    /// A store that answers every call with success.
    fn accepting() -> Arc<Self> {
        Arc::new(Self {
            persisted: Mutex::new(Vec::new()),
            deleted: Mutex::new(Vec::new()),
            refuse_persist: Mutex::new(None),
            refuse_delete: Mutex::new(None),
        })
    }

    /// Makes every later persist answer with `sentence`.
    fn refuse_persist(&self, sentence: &str) {
        *self.refuse_persist.lock().unwrap() = Some(sentence.to_owned());
    }

    /// Makes every later delete answer with `sentence`.
    fn refuse_delete(&self, sentence: &str) {
        *self.refuse_delete.lock().unwrap() = Some(sentence.to_owned());
    }

    /// The newest document stored for `account_id`, the entry a later
    /// launch reads (a real platform store replaces; the script keeps
    /// the history so "newest" stays meaningful).
    fn newest(&self, account_id: &str) -> Option<String> {
        self.persisted
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(id, _)| id == account_id)
            .map(|(_, toml)| toml.clone())
    }

    /// The ids the delete calls named, in order.
    fn deletions(&self) -> Vec<String> {
        self.deleted.lock().unwrap().clone()
    }
}

impl AccountCredentialStore for ScriptedStore {
    fn persist(&self, account_id: &str, config_toml: &str) -> Result<(), CredentialStoreError> {
        if let Some(sentence) = self.refuse_persist.lock().unwrap().clone() {
            return Err(CredentialStoreError(sentence));
        }
        self.persisted
            .lock()
            .unwrap()
            .push((account_id.to_owned(), config_toml.to_owned()));
        Ok(())
    }

    fn delete(&self, account_id: &str) -> Result<(), CredentialStoreError> {
        if let Some(sentence) = self.refuse_delete.lock().unwrap().clone() {
            return Err(CredentialStoreError(sentence));
        }
        self.deleted.lock().unwrap().push(account_id.to_owned());
        self.persisted
            .lock()
            .unwrap()
            .retain(|(id, _)| id != account_id);
        Ok(())
    }
}

/// A transaction over `store`.
fn over(store: &Arc<ScriptedStore>) -> CredentialTransaction {
    CredentialTransaction::new(Arc::clone(store) as Arc<dyn AccountCredentialStore>)
}

/// The account every path moves: one id, so the paths' answers cannot
/// cross-contaminate through the fixtures.
fn account() -> AccountId {
    AccountId::try_from("oauth-account").expect("the account id parses")
}

/// A synthetic grant document: opaque to the transaction, shaped like
/// the real one's rule (the refresh token the single at-rest secret, no
/// access token riding).
fn grant(refresh_token: &str) -> String {
    format!("[oauth]\nrefresh_token = \"{refresh_token}\"\n")
}

#[test]
fn a_synthetic_grant_round_trip_adds_rotates_and_removes() {
    let store = ScriptedStore::accepting();
    let transaction = over(&store);
    let account = account();

    transaction
        .add(&account, &grant("grant-one"))
        .expect("the add persists");
    assert_eq!(store.newest(account.as_str()), Some(grant("grant-one")));

    transaction
        .rotate(&account, &grant("grant-two"))
        .expect("the rotation persists");
    assert_eq!(
        store.newest(account.as_str()),
        Some(grant("grant-two")),
        "the rotated grant replaces what stood; the superseded one is gone"
    );

    transaction.remove(&account).expect("the removal erases");
    assert_eq!(
        store.newest(account.as_str()),
        None,
        "a removed account leaves nothing stored"
    );
}

#[test]
fn an_add_whose_persist_fails_rolls_back_and_answers_the_error() {
    let store = ScriptedStore::accepting();
    store.refuse_persist("the keychain is locked");
    let transaction = over(&store);
    let account = account();

    let refused = transaction
        .add(&account, &grant("grant-one"))
        .expect_err("the refused persist answers");
    assert!(
        refused.to_string().contains("the keychain is locked"),
        "the store's own sentence is the answer: {refused}"
    );
    assert_eq!(
        store.newest(account.as_str()),
        None,
        "nothing stays stored for the account"
    );
    assert_eq!(
        store.deletions(),
        vec![account.as_str().to_owned()],
        "the rollback runs the delete, so a partially landed write cannot leave a half-there account"
    );
}

#[test]
fn a_rotation_whose_persist_fails_logs_what_the_next_launch_faces() {
    let lines = captured();
    let store = ScriptedStore::accepting();
    let transaction = over(&store);
    let account = account();
    transaction
        .add(&account, &grant("grant-one"))
        .expect("the add persists");

    let before = lines.lock().unwrap().len();
    store.refuse_persist("the store is full");
    let refused = transaction
        .rotate(&account, &grant("grant-two"))
        .expect_err("the refused rotation answers");
    assert!(
        refused.to_string().contains("the store is full"),
        "the refusal is answered as well as logged: {refused}"
    );
    assert_eq!(
        store.newest(account.as_str()),
        Some(grant("grant-one")),
        "the previous grant stays stored: what the next launch faces"
    );

    let new = lines.lock().unwrap();
    let rotation_errors = new
        .iter()
        .skip(before)
        .filter(|(level, _)| *level == log::Level::Error)
        .filter(|(_, message)| {
            message.contains(account.as_str()) && message.contains("next launch")
        })
        .count();
    assert!(
        rotation_errors >= 1,
        "the rotation's refusal is logged at error, naming the account and the next launch: {new:?}"
    );
}

#[test]
fn a_removal_whose_delete_fails_answers_the_error_without_reverting() {
    let store = ScriptedStore::accepting();
    let transaction = over(&store);
    let account = account();
    transaction
        .add(&account, &grant("grant-one"))
        .expect("the add persists");

    store.refuse_delete("the credential manager refused");
    let refused = transaction
        .remove(&account)
        .expect_err("the refused delete answers");
    assert!(
        refused
            .to_string()
            .contains("the credential manager refused"),
        "the store's own sentence is the answer: {refused}"
    );
    assert_eq!(
        store.newest(account.as_str()),
        Some(grant("grant-one")),
        "nothing reverts: the entry stands exactly as it was, for better or worse"
    );
}
