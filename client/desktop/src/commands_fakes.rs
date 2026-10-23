// SPDX-License-Identifier: GPL-3.0-only
//! The command-surface test fixtures: one compiled copy of the fake app the
//! command tests drive, the in-memory credential store, and the state builder
//! over them, at the crate root so every command test module (the command
//! tests, the reading tests they split off, and the drafts, sending, compose,
//! and ai tests) includes the same module rather than its own.

// The fake's preset snapshots (the reading and compose answers), a child
// module so this file stays under the 500-line limit; the re-export keeps
// every test module's `commands_fakes::…` paths unchanged.
mod snapshots;

// The fake's whole AppPort impl beside the A3 assist pulls' seeds, its
// own child for the same limit (one trait impl for the one fake, so the
// block cannot split); the re-export keeps the seeds' type nameable
// through the paths the tests already use.
mod port;

// The MCP host fixture (an empty backend over the in-memory ledger in a
// per-test scratch directory), its own child for the same limit, for
// the state builder below and any test that builds a state of its own.
mod mcp_host;

pub(crate) use mcp_host::host;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use astra_app::{
    AttachmentContent, AttachmentReadError, AvatarView, Chip, ComposeIntent, ContactDraftInput,
    DraftResume, DraftsIntent, Intent, OutboxIntent,
};
use astra_viewmodel::{
    CalendarSnapshot, ComposeKind, ComposeRequestSnapshot, DraftSaveState, DraftStatusSnapshot,
    MailboxListSnapshot, MailtoOpen, PeopleSnapshot, ReadingSnapshot, SendingSnapshot,
    SettingsSnapshot, SyncProgressSnapshot,
};
use engine_api::RecipientSuggestion;

use crate::{
    app_state::AppState,
    credential_store::{AccountCredentialStore, CredentialStoreError},
    AccountDials,
};

pub(crate) use snapshots::{compose_prefill, open_snapshot};

pub(crate) use port::{AssistSeeds, BriefSeeds, DocsSeeds};

/// A scratch directory for one test's account index.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("astra-desktop-tests")
        .join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
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

/// The chips the fake's AI pull answers, keyed by the (account, key) pair
/// of the message they belong to.
type SeededChips = Vec<((String, String), Vec<Chip>)>;

/// A fake app core behind the same seam the commands hold: records what it was
/// asked and answers with preset snapshots.
pub(crate) struct FakeApp {
    pub(crate) dispatched: Mutex<Vec<Intent>>,
    pub(crate) added: Mutex<Vec<String>>,
    pub(crate) synced: Mutex<Vec<String>>,
    pub(crate) mailbox_list: MailboxListSnapshot,
    pub(crate) sync_progress: SyncProgressSnapshot,
    pub(crate) reading: Mutex<Option<ReadingSnapshot>>,
    /// The attachment read's answer; `Err` stands in for the typed refusal.
    pub(crate) attachment: Mutex<Result<Option<AttachmentContent>, AttachmentReadError>>,
    /// The (account, key, id) triples the attachment read was asked for, so
    /// a test can assert the command hands the webview's ids through to the
    /// core.
    pub(crate) attachment_into: Mutex<Vec<(String, String, u32)>>,
    /// The draft-status snapshot the pull answers with: what the fake's drafts
    /// dispatches last published.
    pub(crate) draft_status: Mutex<Option<DraftStatusSnapshot>>,
    /// The compose-request snapshot the pull answers with: what the fake's
    /// compose dispatches last published into the single slot.
    pub(crate) compose_request: Mutex<Option<ComposeRequestSnapshot>>,
    /// The mailto open's answer: the prefill a link opens, or `None` when
    /// no account stands.
    pub(crate) mailto_open: Mutex<Option<MailtoOpen>>,
    /// The links the mailto open was asked to open, so a test can assert
    /// the host hands the raw link through to the core.
    pub(crate) mailto_into: Mutex<Vec<String>>,
    /// The stored draft the resume answers with, emptied by a discard.
    pub(crate) stored_draft: Mutex<Option<DraftResume>>,
    /// The (account, composition) pairs the resume was asked to open, so a test
    /// can assert the command hands the webview's ids through to the core.
    pub(crate) resumed_into: Mutex<Vec<(String, String)>>,
    /// The sending snapshot the pull answers with: what the fake's submit and
    /// outbox dispatches last published over its own op counter.
    pub(crate) sending: Mutex<SendingQueue>,
    /// The chips the AI pull answers with, keyed by the message pair; a
    /// dismissal removes the entry.
    pub(crate) chips: Mutex<SeededChips>,
    /// The (account, key) pairs the chips pull was asked about, so a test
    /// can assert the command hands the webview's ids through to the core.
    pub(crate) chips_into: Mutex<Vec<(String, String)>>,
    /// The summary the AI pull answers with; `None` refuses with the
    /// unresolvable-message failure.
    pub(crate) summary: Mutex<Option<String>>,
    /// The (account, key) pairs the summarise pull was asked about.
    pub(crate) summarise_into: Mutex<Vec<(String, String)>>,
    /// The style note the AI pull answers with.
    pub(crate) style_note: Mutex<Option<String>>,
    /// The accounts the style pull was asked about.
    pub(crate) style_into: Mutex<Vec<String>>,
    /// The (account, key) pairs whose chips a dismissal hid.
    pub(crate) dismissed: Mutex<Vec<(String, String)>>,
    /// The (account, key, draft) triples the accept command handed the core,
    /// so a test can assert the drafted reply crosses untouched.
    pub(crate) drafted_into: Mutex<Vec<(String, String, String)>>,
    /// The A3 assist pulls' seeds and their record, one struct under one
    /// lock so a pull and its answer cannot tear.
    pub(crate) assist: Mutex<AssistSeeds>,
    /// The A4 documents pulls' seeds and their record, under the same
    /// one-struct-one-lock discipline.
    pub(crate) docs: Mutex<DocsSeeds>,
    /// The calendar snapshot the pull answers with.
    pub(crate) calendar: Mutex<CalendarSnapshot>,
    /// The calendar brief pull's seeds and their record, under the same
    /// one-struct-one-lock discipline.
    pub(crate) brief: Mutex<BriefSeeds>,
    /// The people snapshot the pull answers with.
    pub(crate) people: Mutex<PeopleSnapshot>,
    /// The card-seed read's answer.
    pub(crate) card_seed: Mutex<Option<ContactDraftInput>>,
    /// The (person, account, card) triples the seed read was asked for, so a
    /// test can assert the command hands the webview's reference through to
    /// the core.
    pub(crate) seeded_into: Mutex<Vec<(u64, String, String)>>,
    /// The suggestions pull's answer.
    pub(crate) suggestions: Mutex<Vec<RecipientSuggestion>>,
    /// The (token, limit) pairs the suggestions pull was asked for.
    pub(crate) suggested_into: Mutex<Vec<(String, usize)>>,
    /// The avatar read's answer.
    pub(crate) avatar: Mutex<AvatarView>,
    /// The addresses the avatar read was asked about.
    pub(crate) avatar_into: Mutex<Vec<String>>,
    /// The settings snapshot the pull answers with.
    pub(crate) settings: Mutex<SettingsSnapshot>,
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
            attachment: Mutex::new(Ok(None)),
            attachment_into: Mutex::new(Vec::new()),
            draft_status: Mutex::new(None),
            compose_request: Mutex::new(None),
            mailto_open: Mutex::new(None),
            mailto_into: Mutex::new(Vec::new()),
            stored_draft: Mutex::new(None),
            resumed_into: Mutex::new(Vec::new()),
            sending: Mutex::new(SendingQueue::default()),
            chips: Mutex::new(Vec::new()),
            chips_into: Mutex::new(Vec::new()),
            summary: Mutex::new(None),
            summarise_into: Mutex::new(Vec::new()),
            style_note: Mutex::new(None),
            style_into: Mutex::new(Vec::new()),
            dismissed: Mutex::new(Vec::new()),
            drafted_into: Mutex::new(Vec::new()),
            assist: Mutex::new(AssistSeeds::default()),
            docs: Mutex::new(DocsSeeds::default()),
            calendar: Mutex::new(CalendarSnapshot::default()),
            brief: Mutex::new(BriefSeeds::default()),
            people: Mutex::new(PeopleSnapshot::default()),
            card_seed: Mutex::new(None),
            seeded_into: Mutex::new(Vec::new()),
            suggestions: Mutex::new(Vec::new()),
            suggested_into: Mutex::new(Vec::new()),
            avatar: Mutex::new(AvatarView::default()),
            avatar_into: Mutex::new(Vec::new()),
            settings: Mutex::new(SettingsSnapshot::default()),
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

    /// The compose family's contract the compose command observes: every
    /// dispatch publishes one request into the single slot, the prefill
    /// shaped by the kind the dispatch named, and nothing publishes `None`
    /// back, so the slot holds the newest request until another replaces it.
    fn play_compose(&self, compose: &ComposeIntent) {
        let (kind, message) = match compose {
            ComposeIntent::Reply { message } => (ComposeKind::Reply, message),
            ComposeIntent::ReplyAll { message } => (ComposeKind::ReplyAll, message),
            ComposeIntent::Forward { message, .. } => (ComposeKind::Forward, message),
            // The family is non-exhaustive; a verb the fake knows no prefill
            // for publishes nothing.
            _ => return,
        };
        *self.compose_request.lock().unwrap() =
            Some(compose_prefill(kind, message.account.as_str()));
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
                    // The gate intents are the app core's seam; the fake never
                    // gates, so they change nothing here.
                    _ => {}
                }
            }
            // Only the outbox family plays; every other intent is the app's,
            // not the fake queue's.
            _ => {}
        }
    }
}

/// An in-memory credential store whose persist and delete can be told to
/// refuse.
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
        if self.refuse {
            return Err(CredentialStoreError(
                "the platform store refused the delete".to_owned(),
            ));
        }
        self.entries
            .lock()
            .unwrap()
            .retain(|(id, _)| id != account_id);
        Ok(())
    }
}

/// A state over a fake app and store, with a real runtime and a scratch index.
/// The agent relay runs the stub sidecar in echo mode (no process starts
/// until an agent command asks for one), and the background client stays
/// off: no tier list or provider key is read out of the test environment.
/// An in-memory crypto store for the host's tests: the wrapper factory and
/// the crypto-method write read one, and no test here dials a real key.
pub(crate) fn crypto_store() -> Arc<astra_crypto_store::CryptoStore> {
    Arc::new(astra_crypto_store::CryptoStore::open_in_memory().expect("the in-memory crypto store"))
}

pub(crate) fn state(name: &str, app: Arc<FakeApp>, store: Arc<FakeStore>) -> AppState {
    let (relay, _sink, _data) = crate::agent::testkit::stub_relay(name, "echo", 3, 50);
    AppState {
        app,
        agent: relay,
        mcp: mcp_host::host(name),
        plugins: Arc::new(crate::plugin::PluginRegistry::new(
            scratch(name).join("plugins"),
            vec![],
            true,
        )),
        llm: None,
        connections: Arc::new(AccountDials::empty(&scratch(name))),
        crypto_store: crypto_store(),
        photo_root: scratch(name).join("photos"),
        credential_store: store,
        oauth: crate::oauth::wiring::offline_surface(),
        refusals: Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
        pending_mailto: Mutex::new(None),
        runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
    }
}
