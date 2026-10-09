// SPDX-License-Identifier: GPL-3.0-only
//! The command-surface test fixtures: one compiled copy of the fake app the
//! command tests drive, the in-memory credential store, and the state builder
//! over them, at the crate root so every command test module (the command tests,
//! the reading tests they split off, and the drafts tests) includes the same
//! module rather than its own.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use astra_app::{AppError, CompositionId, DraftResume, DraftsIntent, Intent, OutboxIntent};
use astra_viewmodel::{
    DraftSaveState, DraftStatusSnapshot, MailboxListSnapshot, ReadingSnapshot, SendingSnapshot,
    SyncProgressSnapshot,
};
use engine_api::{AccountId, ProviderKey};

use crate::{
    app_state::{AppPort, AppState, HostAccount},
    credential_store::{AccountCredentialStore, CredentialStoreError},
    AccountDials,
};

/// A scratch directory for one test's account index.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("astra-desktop-tests")
        .join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// The snapshot the fake app publishes for the message an open names: the same
/// shape astra-viewmodel's wire tests pin, carrying one attachment.
pub(crate) fn open_snapshot() -> ReadingSnapshot {
    ReadingSnapshot {
        message: astra_viewmodel::MessageKey {
            account: AccountId::try_from("work").expect("an account id"),
            key: ProviderKey::new("imap:v1:u42").expect("a provider key"),
        },
        subject: "Quarterly report".to_owned(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        html: Some("<p>See the report.</p>".to_owned()),
        plain: Some("See the report.".to_owned()),
        attachments: vec![astra_viewmodel::AttachmentRow {
            id: 1,
            file_name: "report.pdf".to_owned(),
            media_type: "application/pdf".to_owned(),
            size: 2048,
        }],
        remote_image_count: 2,
    }
}

/// The instant the fake stamps every queued send with, the same whole-second
/// UTC instant the drafts fake fixes its landed save at.
pub(crate) const ARRIVED_AT: &str = "2026-10-09T08:30:00Z";

/// The fake's outbox: the next op id it assigns and the snapshot the pull
/// answers with, under one lock so a dispatch and its publish cannot tear.
#[derive(Default)]
pub(crate) struct SendingQueue {
    next_op: u64,
    snapshot: SendingSnapshot,
}

/// A fake app core behind the same seam the commands hold: records what it was
/// asked and answers with preset snapshots.
pub(crate) struct FakeApp {
    pub(crate) dispatched: Mutex<Vec<Intent>>,
    pub(crate) added: Mutex<Vec<String>>,
    pub(crate) synced: Mutex<Vec<String>>,
    pub(crate) mailbox_list: MailboxListSnapshot,
    pub(crate) sync_progress: SyncProgressSnapshot,
    pub(crate) reading: Mutex<Option<ReadingSnapshot>>,
    /// The draft-status snapshot the pull answers with: what the fake's drafts
    /// dispatches last published.
    pub(crate) draft_status: Mutex<Option<DraftStatusSnapshot>>,
    /// The stored draft the resume answers with, emptied by a discard.
    pub(crate) stored_draft: Mutex<Option<DraftResume>>,
    /// The (account, composition) pairs the resume was asked to open, so a test
    /// can assert the command hands the webview's ids through to the core.
    pub(crate) resumed_into: Mutex<Vec<(String, String)>>,
    /// The sending snapshot the pull answers with: what the fake's submit and
    /// outbox dispatches last published over its own op counter.
    pub(crate) sending: Mutex<SendingQueue>,
}

impl FakeApp {
    pub(crate) fn new() -> Arc<Self> {
        Self::with_snapshots(
            MailboxListSnapshot::default(),
            SyncProgressSnapshot::default(),
        )
    }

    /// A fake whose pull commands answer with the given snapshots.
    pub(crate) fn with_snapshots(
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
            draft_status: Mutex::new(None),
            stored_draft: Mutex::new(None),
            resumed_into: Mutex::new(Vec::new()),
            sending: Mutex::new(SendingQueue::default()),
        })
    }

    /// The drafts contract the drafts commands observe: a save publishes the
    /// in-flight hint and then the landed one (and forgets it when the composer
    /// is leaving), a discard empties both the hint and the stored copy, and a
    /// close empties the hint while the stored copy stays on the server for the
    /// user to find.
    fn play_drafts(&self, drafts: &DraftsIntent) {
        match drafts {
            DraftsIntent::Save {
                composition,
                from,
                to,
                cc,
                bcc,
                subject,
                document,
                then_close,
            } => {
                let body_text = astra_composer::render(document)
                    .map(|output| output.plain_text)
                    .unwrap_or_default();
                *self.stored_draft.lock().unwrap() = Some(DraftResume {
                    account: from.as_str().to_owned(),
                    to: to.clone(),
                    cc: cc.clone(),
                    bcc: bcc.clone(),
                    subject: subject.clone(),
                    body_text,
                });
                let saving = DraftStatusSnapshot {
                    composition: composition.as_str().to_owned(),
                    state: DraftSaveState::Saving,
                    saved_at: String::new(),
                };
                *self.draft_status.lock().unwrap() = Some(saving);
                if *then_close {
                    // A save that closes the composer forgets the composition,
                    // so the hint goes with it.
                    *self.draft_status.lock().unwrap() = None;
                } else {
                    let saved = DraftStatusSnapshot {
                        composition: composition.as_str().to_owned(),
                        state: DraftSaveState::Saved,
                        saved_at: "2026-10-09T08:30:00Z".to_owned(),
                    };
                    *self.draft_status.lock().unwrap() = Some(saved);
                }
            }
            DraftsIntent::Discard { .. } => {
                *self.stored_draft.lock().unwrap() = None;
                *self.draft_status.lock().unwrap() = None;
            }
            DraftsIntent::Close { .. } => {
                *self.draft_status.lock().unwrap() = None;
            }
        }
    }

    /// The outbox contract the sending commands observe: a submit queues a
    /// send under the next op id, a withdrawal or a delivered answer removes
    /// it, a send now marks it in flight, and a not-delivered answer puts it
    /// back in the queue. A show changes nothing: a gesture that changed
    /// nothing says nothing, and the host pulls regardless.
    fn play_sending(&self, intent: &Intent) {
        let mut queue = self.sending.lock().unwrap();
        match intent {
            Intent::SubmitRichMail { from, subject, .. } => {
                queue.next_op += 1;
                let op = queue.next_op;
                queue.snapshot.rows.push(astra_viewmodel::QueuedRow {
                    queued: astra_viewmodel::QueuedKey {
                        account: from.clone(),
                        op,
                    },
                    state: astra_viewmodel::QueuedState::Queued,
                    subject: subject.clone(),
                    arrived_at: ARRIVED_AT.to_owned(),
                });
            }
            Intent::Outbox(action) => {
                let acted = |row: &astra_viewmodel::QueuedRow, queued: &astra_app::QueuedRef| {
                    row.queued.account == queued.account && row.queued.op == queued.op.get()
                };
                match action {
                    OutboxIntent::Show => {}
                    OutboxIntent::Cancel { queued } | OutboxIntent::ConfirmSent { queued } => {
                        queue.snapshot.rows.retain(|row| !acted(row, queued));
                    }
                    OutboxIntent::SendNow { queued } => {
                        for row in &mut queue.snapshot.rows {
                            if acted(row, queued) {
                                row.state = astra_viewmodel::QueuedState::Sending;
                            }
                        }
                    }
                    OutboxIntent::ConfirmNotSent { queued } => {
                        for row in &mut queue.snapshot.rows {
                            if acted(row, queued) {
                                row.state = astra_viewmodel::QueuedState::Queued;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[async_trait::async_trait]
impl AppPort for FakeApp {
    async fn dispatch(&self, intent: Intent) -> Result<(), AppError> {
        // The reading pane's contract: an open publishes the one snapshot, a close empties it.
        match &intent {
            Intent::OpenMessage { .. } => {
                *self.reading.lock().unwrap() = Some(open_snapshot());
            }
            Intent::CloseReading => *self.reading.lock().unwrap() = None,
            _ => {}
        }
        if let Intent::Drafts(drafts) = &intent {
            self.play_drafts(drafts);
        }
        self.play_sending(&intent);
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

    fn draft_status(&self) -> Option<DraftStatusSnapshot> {
        self.draft_status.lock().unwrap().clone()
    }

    fn sending(&self) -> SendingSnapshot {
        self.sending.lock().unwrap().snapshot.clone()
    }

    async fn resume_latest_draft(
        &self,
        account: &AccountId,
        composition: CompositionId,
    ) -> Option<DraftResume> {
        self.resumed_into
            .lock()
            .unwrap()
            .push((account.as_str().to_owned(), composition.as_str().to_owned()));
        self.stored_draft.lock().unwrap().clone()
    }

    async fn add_new_account_deferred(&self, account: HostAccount) {
        self.added
            .lock()
            .unwrap()
            .push(account.id.as_str().to_owned());
    }

    async fn sync_added_account(&self, id: &AccountId) {
        self.synced.lock().unwrap().push(id.as_str().to_owned());
    }
}

/// An in-memory credential store whose persist can be told to refuse.
pub(crate) struct FakeStore {
    pub(crate) refuse: bool,
    pub(crate) entries: Mutex<Vec<(String, String)>>,
}

impl FakeStore {
    pub(crate) fn accepting() -> Arc<Self> {
        Arc::new(Self {
            refuse: false,
            entries: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn refusing() -> Arc<Self> {
        Arc::new(Self {
            refuse: true,
            entries: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn stored(&self, account_id: &str) -> Option<String> {
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
pub(crate) fn state(name: &str, app: Arc<FakeApp>, store: Arc<FakeStore>) -> AppState {
    AppState {
        app,
        runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
        connections: Arc::new(AccountDials::empty(&scratch(name))),
        credential_store: store,
    }
}
