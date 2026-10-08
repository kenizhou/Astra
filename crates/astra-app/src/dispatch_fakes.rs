// SPDX-License-Identifier: GPL-3.0-only
//! Test fixtures for the dispatch-loop tests: the JMAP-shaped [`FakeProvider`], the
//! recording observer, the not-yet connector, and the account/app builders. This
//! crate's own seam, mirroring the per-crate fakes of the port source:
//! astra-account's scripted IMAP server models the wire, this one models the engine's
//! provider contract. Included by `dispatch_tests` via `#[path]`, so not every helper
//! is used by every test and unused items are allowed rather than gated one by one.

#![allow(dead_code)]

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, EmailAddress, Engine, Mailbox, MailboxEdit,
    MailboxEditReceipt, MailboxId, MailboxRole, MailboxWrites, Provider, SyncWindow,
};
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{
    ConnectionInfo, EmailChunk, EmailStream, ProviderError, ProviderResult, ScopeSync,
};

use crate::{Account, App, AppObserver, MailboxConnector, Surface};

/// Records the surfaces the app signals, so a test can assert the loop fired.
pub(super) struct RecordingObserver {
    /// Every surface signalled so far, in order.
    pub(super) surfaces: Arc<Mutex<Vec<Surface>>>,
}

impl AppObserver for RecordingObserver {
    fn surface_changed(&self, surface: Surface) {
        self.surfaces.lock().unwrap().push(surface);
    }
}

/// A minimal JMAP-shaped fake: a fixed set of mailboxes, a snapshot on the first
/// folder-list sync of a scope and an empty delta afterwards, and no mail at all (the
/// pane this milestone renders reads folders, not messages). Counts the email passes
/// it has served, so a test can prove which path drove a sync.
pub(super) struct FakeProvider {
    /// The account's whole tree, served as the first folder-list snapshot.
    pub(super) mailboxes: Vec<Mailbox>,
    /// How many email sync passes this provider has served.
    pub(super) syncs: Arc<AtomicUsize>,
}

impl FakeProvider {
    /// A provider listing `mailboxes` as the account's whole tree.
    pub(super) fn with(mailboxes: Vec<Mailbox>) -> Self {
        Self {
            mailboxes,
            syncs: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl Provider for FakeProvider {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none().with_mail())
    }

    async fn sync_mailboxes(
        &self,
        _account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Mailbox>> {
        if cursor.is_some() {
            return Ok(ScopeSync::new(
                SyncUpdate::delta(Vec::new(), Vec::new()),
                SyncState::new("mbox-2"),
            ));
        }
        let present = self
            .mailboxes
            .iter()
            .map(|mailbox| mailbox.id.key().clone())
            .collect();
        Ok(ScopeSync::new(
            SyncUpdate::snapshot(self.mailboxes.clone(), present),
            SyncState::new("mbox-1"),
        ))
    }

    fn stream_email<'a>(
        &'a self,
        _account: &'a AccountId,
        cursor: Option<&'a SyncState>,
        _window: SyncWindow,
        _fetch_batch: usize,
        _chunk_size: usize,
    ) -> EmailStream<'a> {
        self.syncs.fetch_add(1, Ordering::SeqCst);
        let state = if cursor.is_some() {
            "email-2"
        } else {
            "email-1"
        };
        Box::pin(futures::stream::iter(vec![Ok(EmailChunk::reconcile_last(
            Vec::new(),
            Vec::new(),
            Some(0),
            SyncState::new(state),
        ))]))
    }
}

#[async_trait]
impl MailboxWrites for FakeProvider {
    async fn edit_mailbox(
        &self,
        _account: &AccountId,
        _edit: &MailboxEdit,
    ) -> ProviderResult<MailboxEditReceipt> {
        Err(ProviderError::invalid_state("the fake holds no tree"))
    }
}

impl CalendarWrites for FakeProvider {}

/// The connector the fakes would dial on demand; one that never connects, which is the
/// honest answer until the folder intents exist.
pub(super) struct NoConnector;

#[async_trait]
impl MailboxConnector<FakeProvider> for NoConnector {
    async fn connect_folder(
        &self,
        _account: &AccountId,
        _mailbox_key: &str,
    ) -> Option<FakeProvider> {
        None
    }
}

/// A folder key, the way every fixture names one.
pub(super) fn key(value: &str) -> MailboxId {
    MailboxId::try_from(value).unwrap()
}

/// A plain top-level folder called `name`.
pub(super) fn folder(key_value: &str, name: &str) -> Mailbox {
    Mailbox::new(key(key_value), name)
}

/// The role-bearing Inbox every account's tree leads with.
pub(super) fn inbox() -> Mailbox {
    let mut mailbox = folder("inbox", "Inbox");
    mailbox.role = Some(MailboxRole::Inbox);
    mailbox
}

/// Wraps `provider` as an account `id` (its identity is `me@<id>.local`).
pub(super) fn account(id: &str, provider: FakeProvider) -> Account<FakeProvider> {
    Account {
        id: AccountId::try_from(id).unwrap(),
        providers: vec![provider],
        identity: EmailAddress::new(format!("me@{id}.local")),
    }
}

/// Builds an in-memory app over `accounts`, recording signalled surfaces into
/// `surfaces`.
pub(super) fn app(
    accounts: Vec<Account<FakeProvider>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<FakeProvider> {
    app_with_prefs(accounts, None, surfaces)
}

/// Builds an in-memory app over `accounts` whose preferences persist to `prefs_path`,
/// so a test can prove a choice survives a relaunch (a fresh app over the same path).
pub(super) fn app_with_prefs(
    accounts: Vec<Account<FakeProvider>>,
    prefs_path: Option<PathBuf>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<FakeProvider> {
    App::new(
        Arc::new(Engine::open_in_memory().unwrap()),
        accounts,
        None,
        Arc::new(RecordingObserver {
            surfaces: Arc::clone(surfaces),
        }),
    )
    .with_prefs_path(prefs_path)
}

/// A fresh scratch directory of its own, so one test's writes cannot reach another's.
pub(super) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-dispatch-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// How many folder rows the pane holds across every account.
pub(super) fn pane_rows(snapshot: &astra_viewmodel::MailboxListSnapshot) -> usize {
    snapshot
        .account_folders
        .iter()
        .map(|account| account.folders.len())
        .sum()
}

/// The folder keys the pane draws for `account`, in order.
pub(super) fn folder_keys(
    snapshot: &astra_viewmodel::MailboxListSnapshot,
    account: &str,
) -> Vec<String> {
    snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id.as_str() == account)
        .map(|row| {
            row.folders
                .iter()
                .map(|folder| folder.key.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default()
}
