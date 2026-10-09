// SPDX-License-Identifier: GPL-3.0-only
//! The shell's managed state: the app core the command surface drives, the runtime
//! it drives on, the per-account dial table, and the credential store. One value,
//! registered once at boot, read back by every command through Tauri's `State`.

use std::sync::Arc;

use astra_app::{Account, App, CompositionId, DraftResume, Intent};
use astra_viewmodel::{
    DraftStatusSnapshot, MailboxListSnapshot, ReadingSnapshot, SendingSnapshot,
    SyncProgressSnapshot,
};
use async_trait::async_trait;
use engine_api::{AccountId, Provider};

use crate::{AccountCredentialStore, AccountDials, AppRuntime};

/// The concrete provider parameter the desktop host runs the app over:
/// `App<HostProvider>`.
///
/// Every element is a `ReconnectingImapProvider` over the TLS IMAP transport (an
/// IMAP account's connections bound folder by folder, retried once after a dropped
/// socket), boxed at `astra-account`'s public seam: `connect_mail_providers` and
/// `connect_imap_mailbox` hand back `Box<dyn Provider>`, and the engine implements
/// its `Provider` trait for `Box<P>`, so the box **is** the host's provider type.
/// Nothing else dials: this host's every account is IMAP in SP1.
pub(crate) type HostProvider = Box<dyn Provider>;

/// One configured account as the host installs it: [`astra_app::Account`] over the
/// host's provider type.
pub(crate) type HostAccount = Account<HostProvider>;

/// The app core the commands drive, behind the one seam the command surface holds.
///
/// Production installs `App<HostProvider>` here. A test installs a fake over the
/// same trait, so the command surface is exercised against the contract rather than
/// a dialled server: what was dispatched, what was pulled, what an add registered.
#[async_trait]
pub(crate) trait AppPort: Send + Sync {
    /// Handles one inbound intent; the changed surface is signalled on completion.
    ///
    /// # Errors
    ///
    /// Returns the app's own error for an intent family this milestone does not
    /// implement.
    async fn dispatch(&self, intent: Intent) -> Result<(), astra_app::AppError>;

    /// The published mailbox-list snapshot, for the host's pull.
    fn mailbox_list(&self) -> MailboxListSnapshot;

    /// The published sync-progress snapshot, for the host's pull.
    fn sync_progress(&self) -> SyncProgressSnapshot;

    /// The published reading snapshot, for the host's pull: the one open
    /// message's body, or `None` when nothing is open.
    fn reading(&self) -> Option<ReadingSnapshot>;

    /// The published draft-status snapshot, for the host's pull: the composition
    /// whose save moved and how it ended, or `None` when no composition has a
    /// save to report.
    fn draft_status(&self) -> Option<DraftStatusSnapshot>;

    /// The published sending snapshot, for the host's pull: every account's
    /// sends that have not gone out, or nothing outstanding.
    fn sending(&self) -> SendingSnapshot;

    /// Opens the account's latest stored draft into `composition`, so the
    /// composer that holds it saves over that copy rather than beside it; the
    /// webview's resume read. `None` when the account holds no draft or its body
    /// cannot be read, in which case nothing is adopted either.
    async fn resume_latest_draft(
        &self,
        account: &AccountId,
        composition: CompositionId,
    ) -> Option<DraftResume>;

    /// Registers a newly added account without syncing, so the pane shows it and the
    /// setup flow can dismiss at once.
    async fn add_new_account_deferred(&self, account: HostAccount);

    /// Syncs one account's mail in the background; the host spawns this after the
    /// install.
    async fn sync_added_account(&self, id: &AccountId);
}

#[async_trait]
impl AppPort for App<HostProvider> {
    async fn dispatch(&self, intent: Intent) -> Result<(), astra_app::AppError> {
        App::dispatch(self, intent).await
    }

    fn mailbox_list(&self) -> MailboxListSnapshot {
        App::mailbox_list(self)
    }

    fn sync_progress(&self) -> SyncProgressSnapshot {
        App::sync_progress(self)
    }

    fn reading(&self) -> Option<ReadingSnapshot> {
        App::reading(self)
    }

    fn draft_status(&self) -> Option<DraftStatusSnapshot> {
        App::draft_status(self)
    }

    fn sending(&self) -> SendingSnapshot {
        App::sending(self)
    }

    async fn resume_latest_draft(
        &self,
        account: &AccountId,
        composition: CompositionId,
    ) -> Option<DraftResume> {
        App::resume_latest_draft(self, account, composition).await
    }

    async fn add_new_account_deferred(&self, account: HostAccount) {
        App::add_new_account_deferred(self, account).await;
    }

    async fn sync_added_account(&self, id: &AccountId) {
        App::sync_added_account(self, id).await;
    }
}

/// The state the shell manages: everything a command needs, in one registered value.
pub(crate) struct AppState {
    /// The app core behind the command surface.
    pub(crate) app: Arc<dyn AppPort>,
    /// The app's async runtime: dispatches, first syncs and blocking dials run on it.
    pub(crate) runtime: AppRuntime,
    /// Every registered account's config and connection pool: what a re-dial, a
    /// folder bind, and the boot's restore read.
    pub(crate) connections: Arc<AccountDials>,
    /// The OS secure store the account-add flow persists through.
    pub(crate) credential_store: Arc<dyn AccountCredentialStore>,
}

impl core::fmt::Debug for AppState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The app core and the pools may hold sensitive handles; count, do not show.
        f.debug_struct("AppState")
            .field("accounts", &self.connections.ids().len())
            .finish_non_exhaustive()
    }
}
