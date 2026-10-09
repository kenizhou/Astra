// SPDX-License-Identifier: GPL-3.0-only
//! Test fixtures for the dispatch-loop tests: the JMAP-shaped [`FakeProvider`], the
//! recording observer, the not-yet connector, and the account/app builders. This
//! crate's own seam, mirroring the per-crate fakes of the port source:
//! astra-account's scripted IMAP server models the wire, this one models the engine's
//! provider contract. A crate-root module compiled once under `cfg(test)`, so the
//! dispatch, list, and reading test modules all use the one copy; not every helper
//! is used by every test and unused items are allowed rather than gated one by one.

#![allow(dead_code)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, Draft, EmailAddress, Engine, Mailbox, MailboxEdit,
    MailboxEditReceipt, MailboxId, MailboxRole, MailboxWrites, Message, Provider, ProviderKey,
    SyncWindow, SystemKeyword, UtcDateTime,
};
use engine_core::raw::RawMime;
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{
    ConnectionInfo, EmailChunk, EmailStream, MailEdit, MailEditReceipt, ProviderError,
    ProviderResult, ScopeSync,
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
/// folder-list sync of a scope and an empty delta afterwards, a fixed set of
/// messages served on the first email pass of a scope (and merely re-presented
/// afterwards, so a re-sync never undoes a local edit), and an `edit_mail` that
/// records every call and answers success. Counts the email passes it has
/// served, so a test can prove which path drove a sync. Raw RFC 5322 sources are
/// served by provider key for the reading pipeline's body reads.
pub(super) struct FakeProvider {
    /// The account's whole tree, served as the first folder-list snapshot.
    pub(super) mailboxes: Vec<Mailbox>,
    /// The account's whole mailbox, served as the first email pass and re-presented
    /// as the present set afterwards. Behind a mutex so a test can model the
    /// server's copy moving between passes (a message deleted server-side).
    pub(super) messages: Arc<Mutex<Vec<Message>>>,
    /// Messages a later email pass delivers as changed, so a refresh test can move
    /// the store's copy of a message the way a server delta would. Empty by
    /// default: a later pass re-presents and changes nothing.
    pub(super) updates: Arc<Mutex<Vec<Message>>>,
    /// Raw RFC 5322 sources by provider key, served through `fetch_message_source`,
    /// so the reading pipeline's body, inline-part, and attachment reads have bytes
    /// to decode. A message without one fails its source fetch. Behind a mutex because
    /// a draft the provider stores registers its source as it goes out.
    pub(super) sources: Mutex<HashMap<String, Vec<u8>>>,
    /// How many email sync passes this provider has served.
    pub(super) syncs: Arc<AtomicUsize>,
    /// Every `edit_mail` this provider was asked to apply, in order.
    pub(super) edits: Arc<Mutex<Vec<MailEdit>>>,
    /// Every draft this provider was asked to store, each with the key that save said
    /// it was superseding. The pair, not just the draft: whether a save names the copy
    /// it replaces is the whole draft contract, and a log of drafts alone cannot tell a
    /// correct save from one storing a second copy.
    pub(super) draft_puts: DraftPutLog,
    /// Every stored draft this provider was asked to remove.
    pub(super) draft_deletes: Arc<Mutex<Vec<ProviderKey>>>,
    /// Draft saves left to refuse as retryable ("no route to host"), so a test can model
    /// an outage the outbox rides out.
    offline_saves: Mutex<u32>,
}

/// Every draft save the fake was asked to store, each `(draft, replacing)` pair.
type DraftPutLog = Arc<Mutex<Vec<(Draft, Option<ProviderKey>)>>>;

impl FakeProvider {
    /// A provider listing `mailboxes` as the account's whole tree, holding no mail.
    pub(super) fn with(mailboxes: Vec<Mailbox>) -> Self {
        Self::with_mail(mailboxes, Vec::new())
    }

    /// A provider whose account also holds `messages`, so the list reads have rows
    /// to project.
    pub(super) fn with_mail(mailboxes: Vec<Mailbox>, messages: Vec<Message>) -> Self {
        Self::with_sources(mailboxes, messages, Vec::new())
    }

    /// [`with_mail`](Self::with_mail) whose messages also carry raw `sources`, each
    /// `(provider key, raw RFC 5322 bytes)`, so a reading open has a body to decode.
    pub(super) fn with_sources(
        mailboxes: Vec<Mailbox>,
        messages: Vec<Message>,
        sources: Vec<(String, Vec<u8>)>,
    ) -> Self {
        Self {
            mailboxes,
            messages: Arc::new(Mutex::new(messages)),
            updates: Arc::new(Mutex::new(Vec::new())),
            sources: Mutex::new(sources.into_iter().collect()),
            syncs: Arc::new(AtomicUsize::new(0)),
            edits: Arc::new(Mutex::new(Vec::new())),
            draft_puts: Arc::new(Mutex::new(Vec::new())),
            draft_deletes: Arc::new(Mutex::new(Vec::new())),
            offline_saves: Mutex::new(0),
        }
    }

    /// A provider whose next `saves` draft saves fail retryably and which then works:
    /// the outage a queued draft save is supposed to ride out.
    pub(super) fn saving_nothing_for(saves: u32) -> Self {
        let provider = Self::with(vec![inbox()]);
        *provider.offline_saves.lock().unwrap() = saves;
        provider
    }

    /// Every recorded draft save, each `(draft, replacing)`, for the contract assertions.
    pub(super) fn draft_puts(&self) -> DraftPutLog {
        Arc::clone(&self.draft_puts)
    }

    /// Every recorded draft removal, for the discard assertions.
    pub(super) fn draft_deletes(&self) -> Arc<Mutex<Vec<ProviderKey>>> {
        Arc::clone(&self.draft_deletes)
    }

    /// Records that the server now holds `draft` under `key`, so the account's next
    /// sync pass presents it to the store the way a real Drafts folder would.
    fn store_draft(&self, key: &str, draft: &Draft) {
        let mut message = Message::new(
            engine_core::ids::MessageId::try_from(key).unwrap(),
            engine_core::membership::Memberships::of_one(MailboxId::try_from("drafts").unwrap()),
        );
        message
            .keywords
            .insert(engine_api::Keyword::system(SystemKeyword::Draft));
        message.envelope.subject = Some(draft.subject.clone());
        message.envelope.from = vec![draft.from.clone()];
        message.envelope.to = draft.to.clone();
        message.envelope.cc = draft.cc.clone();
        message.envelope.message_id = vec![draft.message_id.clone()];
        let puts = self.draft_puts.lock().unwrap().len();
        message.received_at =
            Some(UtcDateTime::new(2026, 10, 9, 9, u8::try_from(puts).unwrap(), 0).unwrap());
        let source = format!(
            "From: {}\r\nTo: {}\r\nSubject: {}\r\n\r\n{}",
            draft.from.email,
            draft
                .to
                .iter()
                .map(|to| to.email.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            draft.subject,
            draft.text_body,
        );
        self.messages.lock().unwrap().push(message.clone());
        self.updates.lock().unwrap().push(message);
        self.sources
            .lock()
            .unwrap()
            .insert(key.to_owned(), source.into_bytes());
    }
}

#[async_trait]
impl Provider for FakeProvider {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none().with_mail().with_mail_drafts())
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
        let messages = self.messages.lock().unwrap().clone();
        // The first pass delivers the whole mailbox; a later pass delivers whatever
        // the test queued as a delta and re-presents the keys it still holds with
        // nothing else changed, so a reconcile never tombstones what a local edit
        // moved (the store shows queued keyword changes until the server answers).
        let changed = if cursor.is_some() {
            self.updates.lock().unwrap().clone()
        } else {
            messages.clone()
        };
        let present = messages
            .iter()
            .map(|message| message.id.key().clone())
            .collect();
        Box::pin(futures::stream::iter(vec![Ok(EmailChunk::reconcile_last(
            changed,
            present,
            Some(messages.len()),
            SyncState::new(state),
        ))]))
    }

    async fn fetch_message_source(
        &self,
        _account: &AccountId,
        message: &Message,
    ) -> ProviderResult<RawMime> {
        self.sources
            .lock()
            .unwrap()
            .get(message.id.key().as_str())
            .cloned()
            .map(RawMime::new)
            .ok_or_else(|| ProviderError::invalid_state("the fake holds no source for that key"))
    }

    async fn edit_mail(
        &self,
        _account: &AccountId,
        edit: &MailEdit,
    ) -> ProviderResult<MailEditReceipt> {
        self.edits.lock().unwrap().push(edit.clone());
        Ok(MailEditReceipt::new(edit.target().clone()))
    }

    /// Stores a draft, answering with a key that **moves on every save**, which is what
    /// three of the four real adapters do. A fake echoing the key back would let a caller
    /// that never passes `replacing` pass this suite and store a second copy per save
    /// against a real server. The stored copy joins the server's mailbox, so a later sync
    /// pass presents it to the store and a resume can read it back.
    async fn put_draft(
        &self,
        _account: &AccountId,
        draft: &Draft,
        replacing: Option<&ProviderKey>,
    ) -> ProviderResult<ProviderKey> {
        {
            let mut offline = self.offline_saves.lock().unwrap();
            if *offline > 0 {
                *offline -= 1;
                return Err(ProviderError::retryable("no route to host"));
            }
        }
        let key = {
            let mut puts = self.draft_puts.lock().unwrap();
            puts.push((draft.clone(), replacing.cloned()));
            ProviderKey::new(format!("draft-{}", puts.len())).unwrap()
        };
        self.store_draft(key.as_str(), draft);
        Ok(key)
    }

    /// Removes a stored draft from the server's mailbox along with its source, the way a
    /// real Drafts folder would, so a later pass reconciles it away.
    async fn delete_draft(&self, _account: &AccountId, draft: &ProviderKey) -> ProviderResult<()> {
        self.draft_deletes.lock().unwrap().push(draft.clone());
        self.messages
            .lock()
            .unwrap()
            .retain(|message| message.id.key() != draft);
        self.updates
            .lock()
            .unwrap()
            .retain(|message| message.id.key() != draft);
        self.sources.lock().unwrap().remove(draft.as_str());
        Ok(())
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

/// A message `key` filed in `mailbox` about `subject`, from Ada, delivered at a fixed
/// base instant, unread and unflagged. Every list fixture starts here and mutates the
/// axis under test.
pub(super) fn mail(key: &str, mailbox: &str, subject: &str) -> Message {
    let mut message = Message::new(
        engine_core::ids::MessageId::try_from(key).unwrap(),
        engine_core::membership::Memberships::of_one(MailboxId::try_from(mailbox).unwrap()),
    );
    message.envelope.subject = Some(subject.to_owned());
    message.envelope.from = vec![EmailAddress::named("Ada Lovelace", "ada@example.test")];
    message.received_at = Some(engine_api::UtcDateTime::new(2026, 10, 9, 8, 0, 0).unwrap());
    message
}

/// Delivers `message` at `hour` past the fixture base instant, so list order is fixed
/// without depending on store insertion order.
pub(super) fn delivered_at(mut message: Message, hour: u8, minute: u8) -> Message {
    message.received_at = Some(engine_api::UtcDateTime::new(2026, 10, 9, hour, minute, 0).unwrap());
    message
}

/// Marks `message` read (the `$seen` keyword), the state a mark-read flips away from.
pub(super) fn read(mut message: Message) -> Message {
    message
        .keywords
        .insert(engine_api::Keyword::system(engine_api::SystemKeyword::Seen));
    message
}

/// Flags `message` (the `$flagged` keyword).
pub(super) fn flagged(mut message: Message) -> Message {
    message.keywords.insert(engine_api::Keyword::system(
        engine_api::SystemKeyword::Flagged,
    ));
    message
}

/// Puts `message` on provider-assigned thread `thread`, so several messages group into
/// one conversation the way a JMAP server threads them.
pub(super) fn threaded(mut message: Message, thread: &str) -> Message {
    message.thread = Some(engine_core::mail::ThreadRef::provider_assigned(
        engine_core::ids::ThreadId::try_from(thread).unwrap(),
    ));
    message
}

/// The message-row keys a snapshot lists, in display order; the shape every list
/// assertion reads before it looks at any row's fields.
pub(super) fn row_keys(snapshot: &astra_viewmodel::MailboxListSnapshot) -> Vec<String> {
    snapshot
        .rows
        .iter()
        .map(|row| match &row.key {
            astra_viewmodel::RowKey::Message { key, .. } => key.as_str().to_owned(),
            astra_viewmodel::RowKey::Thread { thread_id, .. } => thread_id.as_str().to_owned(),
        })
        .collect()
}

/// The one row a snapshot lists under `key`; panics when it is absent, which is the
/// assertion.
pub(super) fn row<'a>(
    snapshot: &'a astra_viewmodel::MailboxListSnapshot,
    key: &str,
) -> &'a astra_viewmodel::MessageRowView {
    snapshot
        .rows
        .iter()
        .find(|row| match &row.key {
            astra_viewmodel::RowKey::Message { key: row_key, .. } => row_key.as_str() == key,
            astra_viewmodel::RowKey::Thread { thread_id, .. } => thread_id.as_str() == key,
        })
        .unwrap_or_else(|| panic!("no row keyed {key}"))
}
