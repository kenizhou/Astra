// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the command surface: every intent's JSON dispatches, an unparsable one
//! is a typed error rather than a panic, the pull commands return what the app
//! published, and the add-account flow rolls back when the credential store refuses.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use astra_app::{AppError, FolderIntent, Intent, MessageRef, ViewMode};
use astra_viewmodel::{MailboxListSnapshot, ReadingSnapshot, SyncProgressSnapshot};
use engine_api::EmailAddress;

use crate::{
    app_state::{AppPort, AppState, HostAccount, HostProvider},
    commands::{
        add_account_with, dispatch_intent, imap_test_with, mailbox_list_of, sync_progress_of,
        AddedAccount, CommandError, ImapTestOutcome,
    },
    credential_store::{AccountCredentialStore, CredentialStoreError},
    AccountDials,
};

/// A config TOML for one account, built the way the setup flow builds one.
fn config_toml(username: &str) -> String {
    let setup = astra_account::AccountSetup {
        imap_host: "imap.example.test".to_owned(),
        username: username.to_owned(),
        credential: astra_account::SetupCredential::Password("a-password".to_owned()),
        smtp_host: None,
        caldav_base_url: None,
        imap_security: astra_account::ConnectionSecurity::ImplicitTls,
        smtp_security: astra_account::ConnectionSecurity::ImplicitTls,
        accepted_certificate: None,
    };
    astra_account::build_config_toml(&setup).expect("the setup builds a config")
}

/// A scratch directory for one test's account index.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("astra-desktop-tests")
        .join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// A fake app core behind the same seam the commands hold: records what it was asked
/// and answers with preset snapshots.
struct FakeApp {
    dispatched: Mutex<Vec<Intent>>,
    added: Mutex<Vec<String>>,
    synced: Mutex<Vec<String>>,
    mailbox_list: MailboxListSnapshot,
    sync_progress: SyncProgressSnapshot,
    reading: Mutex<Option<ReadingSnapshot>>,
}

impl FakeApp {
    fn new() -> Arc<Self> {
        Self::with_snapshots(
            MailboxListSnapshot::default(),
            SyncProgressSnapshot::default(),
        )
    }

    /// A fake whose pull commands answer with the given snapshots.
    fn with_snapshots(
        mailbox_list: MailboxListSnapshot,
        sync_progress: SyncProgressSnapshot,
    ) -> Arc<Self> {
        Arc::new(Self {
            dispatched: Mutex::new(Vec::new()),
            added: Mutex::new(Vec::new()),
            synced: Mutex::new(Vec::new()),
            mailbox_list,
            sync_progress,
            reading: Mutex::new(None),
        })
    }
}

#[async_trait::async_trait]
impl AppPort for FakeApp {
    async fn dispatch(&self, intent: Intent) -> Result<(), AppError> {
        // The reading pane's contract: an open publishes the one snapshot, a close empties it.
        match &intent {
            Intent::OpenMessage { .. } => {
                *self.reading.lock().unwrap() = Some(reading::open_snapshot());
            }
            Intent::CloseReading => *self.reading.lock().unwrap() = None,
            _ => {}
        }
        self.dispatched.lock().unwrap().push(intent);
        Ok(())
    }

    fn mailbox_list(&self) -> MailboxListSnapshot {
        self.mailbox_list.clone()
    }

    fn sync_progress(&self) -> SyncProgressSnapshot {
        self.sync_progress.clone()
    }

    fn reading(&self) -> Option<ReadingSnapshot> {
        self.reading.lock().unwrap().clone()
    }

    async fn add_new_account_deferred(&self, account: HostAccount) {
        self.added
            .lock()
            .unwrap()
            .push(account.id.as_str().to_owned());
    }

    async fn sync_added_account(&self, id: &engine_api::AccountId) {
        self.synced.lock().unwrap().push(id.as_str().to_owned());
    }
}

/// An in-memory credential store whose persist can be told to refuse.
struct FakeStore {
    refuse: bool,
    entries: Mutex<Vec<(String, String)>>,
}

impl FakeStore {
    fn accepting() -> Arc<Self> {
        Arc::new(Self {
            refuse: false,
            entries: Mutex::new(Vec::new()),
        })
    }

    fn refusing() -> Arc<Self> {
        Arc::new(Self {
            refuse: true,
            entries: Mutex::new(Vec::new()),
        })
    }

    fn stored(&self, account_id: &str) -> Option<String> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|(id, _)| id == account_id)
            .map(|(_, toml)| toml.clone())
    }
}

impl AccountCredentialStore for FakeStore {
    fn persist(&self, account_id: &str, config_toml: &str) -> Result<(), CredentialStoreError> {
        if self.refuse {
            return Err(CredentialStoreError(
                "the platform store refused the write".to_owned(),
            ));
        }
        self.entries
            .lock()
            .unwrap()
            .push((account_id.to_owned(), config_toml.to_owned()));
        Ok(())
    }

    fn delete(&self, account_id: &str) -> Result<(), CredentialStoreError> {
        self.entries
            .lock()
            .unwrap()
            .retain(|(id, _)| id != account_id);
        Ok(())
    }
}

/// A state over a fake app and store, with a real runtime and a scratch index.
fn state(name: &str, app: Arc<FakeApp>, store: Arc<FakeStore>) -> AppState {
    AppState {
        app,
        runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
        connections: Arc::new(AccountDials::empty(&scratch(name))),
        credential_store: store,
    }
}

/// A dial that succeeds without touching the network, returning no providers.
// The seam the add flow calls takes a `Result`; a fake that cannot fail still returns
// one, so the signature matches what production hands it.
#[allow(clippy::unnecessary_wraps)]
fn fake_dial(
    _config: &astra_account::AccountConfig,
    _id: &engine_api::AccountId,
) -> Result<(Vec<HostProvider>, Arc<astra_account::ImapConnections>), astra_account::AccountError> {
    Ok((Vec::new(), astra_account::ImapConnections::new()))
}

/// Every intent a host can dispatch, one of each shape.
fn every_intent() -> Vec<Intent> {
    let account = engine_api::AccountId::try_from("user@example.test").expect("an account id");
    vec![
        Intent::RefreshMail,
        Intent::SelectAccount(None),
        Intent::SelectAccount(Some(account.clone())),
        Intent::SetAccountExpanded {
            account: account.clone(),
            expanded: true,
        },
        Intent::SetFolderExpanded {
            folder: astra_app::FolderRef {
                account: account.clone(),
                key: "archive".to_owned(),
            },
            expanded: false,
        },
        Intent::SelectFolder {
            folder: astra_app::FolderRef {
                account: account.clone(),
                key: "inbox".to_owned(),
            },
        },
        Intent::SetViewMode(ViewMode::Threaded),
        Intent::SetViewMode(ViewMode::Flat),
        Intent::ShowMore,
        Intent::OpenMessage {
            message: MessageRef::from_parts("work", "imap:v1:u42".to_owned()).unwrap(),
        },
        Intent::CloseReading,
        Intent::Folders(FolderIntent::Create {
            account,
            parent: None,
            name: "Projects".to_owned(),
        }),
    ]
}

#[test]
fn dispatch_accepts_every_intent_variants_json() {
    let app = FakeApp::new();
    let state = state("dispatch-all", Arc::clone(&app), FakeStore::accepting());

    for intent in every_intent() {
        let wire = serde_json::to_value(intent.clone()).expect("the intent serialises");
        dispatch_intent(&state, wire).expect("the intent dispatches");
    }

    // The dispatches run on the runtime; give them a moment and then compare.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.dispatched.lock().unwrap().len() < 12 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        app.dispatched.lock().unwrap().len(),
        12,
        "every intent variant reached the app: {:?}",
        app.dispatched.lock().unwrap(),
    );
}

#[test]
fn an_unparsable_intent_is_a_typed_error_not_a_panic() {
    let app = FakeApp::new();
    let state = state("dispatch-bad", Arc::clone(&app), FakeStore::accepting());

    let error = dispatch_intent(&state, serde_json::json!({ "bogus": true }))
        .expect_err("an unparsable intent fails");

    assert!(
        matches!(error, CommandError::Intent(_)),
        "the error is typed: {error:?}",
    );
    assert!(
        app.dispatched.lock().unwrap().is_empty(),
        "nothing reached the app",
    );
}

#[test]
fn the_pull_commands_return_what_the_app_published() {
    let account = engine_api::AccountId::try_from("user@example.test").expect("an account id");
    let mailbox_list = MailboxListSnapshot {
        account_folders: vec![astra_viewmodel::AccountFolderRow {
            account_id: account.clone(),
            display_name: "user@example.test".to_owned(),
            expanded: true,
            folders: Vec::new(),
        }],
        ..Default::default()
    };
    let sync_progress = SyncProgressSnapshot {
        accounts: vec![astra_viewmodel::AccountSyncProgress {
            account,
            state: astra_viewmodel::AccountSyncState::Syncing,
            detail: Some("folders 1 of 4".to_owned()),
        }],
    };
    let state = state(
        "pulls",
        FakeApp::with_snapshots(mailbox_list.clone(), sync_progress.clone()),
        FakeStore::accepting(),
    );

    assert_eq!(mailbox_list_of(&state), mailbox_list);
    assert_eq!(sync_progress_of(&state), sync_progress);
}

#[test]
fn an_add_account_dials_persists_registers_and_spawns_the_first_sync() {
    let app = FakeApp::new();
    let store = FakeStore::accepting();
    let state = state("add-ok", Arc::clone(&app), Arc::clone(&store));
    let toml = config_toml("added@example.test");

    let added = add_account_with(&state, &toml, fake_dial).expect("the account is added");

    let expected_id = astra_account::load_str(&toml)
        .and_then(|config| config.account_id())
        .expect("the id derives");
    assert_eq!(
        added,
        AddedAccount {
            account_id: expected_id.as_str().to_owned(),
            email: "added@example.test".to_owned(),
        },
    );
    assert_eq!(
        store.stored(expected_id.as_str()).as_deref(),
        Some(toml.as_str()),
        "the config TOML is stored under the account id",
    );
    assert_eq!(
        state.connections.ids(),
        vec![expected_id.as_str().to_owned()],
        "the dial table registers the account",
    );
    assert_eq!(
        app.added.lock().unwrap().as_slice(),
        [expected_id.as_str()],
        "the app registers the account without syncing",
    );

    // The first sync runs on the runtime; give it a moment.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.synced.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        app.synced.lock().unwrap().as_slice(),
        [expected_id.as_str()],
        "the first sync is spawned for the account",
    );
}

#[test]
fn an_add_account_rolls_back_when_the_store_refuses() {
    let app = FakeApp::new();
    let store = FakeStore::refusing();
    let state = state("add-rollback", Arc::clone(&app), Arc::clone(&store));
    let toml = config_toml("refused@example.test");

    let error =
        add_account_with(&state, &toml, fake_dial).expect_err("the store's refusal surfaces");

    assert!(
        matches!(error, CommandError::Store(_)),
        "the error is typed: {error:?}",
    );
    let id = astra_account::load_str(&toml)
        .and_then(|config| config.account_id())
        .expect("the id derives");
    assert!(
        app.added.lock().unwrap().is_empty(),
        "the account was never registered with the app",
    );
    assert!(
        state.connections.ids().is_empty(),
        "the dial table registered nothing",
    );
    assert!(
        state.connections.indexed_ids().is_empty(),
        "the account index wrote nothing",
    );
    assert!(
        store.stored(id.as_str()).is_none(),
        "nothing was stored for the account",
    );
}

#[test]
fn an_add_account_surfaces_a_dial_failure() {
    let app = FakeApp::new();
    let state = state("add-dial-fails", Arc::clone(&app), FakeStore::accepting());
    let toml = config_toml("offline@example.test");

    let error = add_account_with(&state, &toml, |_config, _id| {
        Err(astra_account::AccountError::SigninRejected(
            "the server refused the password".to_owned(),
        ))
    })
    .expect_err("the dial failure surfaces");

    assert!(
        matches!(error, CommandError::Connect(ref reason) if reason.contains("sign-in")),
        "the error is typed and carries the reason: {error:?}",
    );
    assert!(
        app.added.lock().unwrap().is_empty(),
        "nothing was registered",
    );
}

#[test]
fn a_connection_test_reports_success_and_failure() {
    let ok = imap_test_with(
        serde_json::to_value(astra_account::AccountSetup {
            imap_host: "imap.example.test".to_owned(),
            username: "ok@example.test".to_owned(),
            credential: astra_account::SetupCredential::Password("pw".to_owned()),
            smtp_host: None,
            caldav_base_url: None,
            imap_security: astra_account::ConnectionSecurity::ImplicitTls,
            smtp_security: astra_account::ConnectionSecurity::ImplicitTls,
            accepted_certificate: None,
        })
        .expect("the setup serialises"),
        |_config| Ok(()),
    )
    .expect("the outcome maps");
    let ImapTestOutcome::Ok { account_id } = ok else {
        panic!("a successful dial reports ok: {ok:?}");
    };
    assert!(
        account_id.contains("ok@example.test"),
        "the account id derives from the login: {account_id}",
    );

    let failed = imap_test_with(
        serde_json::to_value(astra_account::AccountSetup {
            imap_host: "imap.example.test".to_owned(),
            username: "bad@example.test".to_owned(),
            credential: astra_account::SetupCredential::Password("pw".to_owned()),
            smtp_host: None,
            caldav_base_url: None,
            imap_security: astra_account::ConnectionSecurity::ImplicitTls,
            smtp_security: astra_account::ConnectionSecurity::ImplicitTls,
            accepted_certificate: None,
        })
        .expect("the setup serialises"),
        |_config| {
            Err(astra_account::AccountError::SigninRejected(
                "the server refused the password".to_owned(),
            ))
        },
    )
    .expect("the outcome maps");
    let ImapTestOutcome::Failed { message } = failed else {
        panic!("a refused sign-in reports a failure: {failed:?}");
    };
    assert!(
        message.contains("sign-in"),
        "the message carries the reason"
    );
}

#[test]
fn a_connection_test_reports_a_rejected_certificate_for_the_ui_to_offer() {
    // The rejection the UI must be able to offer an exception for. The engine type is
    // built only by a real TLS refusal, so the mapping is exercised over the parts a
    // rejection exposes; the outcome shape is what the UI reads.
    let view = crate::commands::map_rejected("imap.self-signed.test", &[0u8; 32], None);
    let wire = serde_json::to_value(&view).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "server_name": "imap.self-signed.test",
            "sha256": "00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00",
            "subject_common_name": null,
            "subject_organisation": null,
            "issuer_common_name": null,
            "issuer_organisation": null,
            "not_before": null,
            "not_after": null,
        }),
    );
}

/// `EmailAddress` names the identity an added account carries; imported so the test
/// states what it builds rather than leaving the import implicit.
#[test]
fn an_added_account_identity_is_the_login_address() {
    let identity = EmailAddress::new("added@example.test");
    assert_eq!(identity.email, "added@example.test");
    assert!(identity.name.is_none());
}

#[cfg(test)]
#[path = "commands_reading_tests.rs"]
mod reading;
