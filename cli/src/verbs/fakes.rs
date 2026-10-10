// SPDX-License-Identifier: GPL-3.0-only
//! The per-verb tests' fixture provider and seeding helpers, mirroring the
//! app's own `dispatch_fakes`: a JMAP-shaped fake whose whole account is one
//! scope, so the engine's sync path fills the in-memory store the way real
//! mail would. Compiled once under `cfg(test)` for every verb's tests; not
//! every helper is used by every test, so unused items are allowed.

#![allow(dead_code)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, Draft, EmailAddress, Engine, Keyword, Mailbox,
    MailboxId, MailboxRole, MailboxWrites, Message, Provider, ProviderKey, StreamTuning,
    SyncCommit, SyncObserver, SyncWindow, SystemKeyword, UtcDateTime,
};
use engine_core::ids::MessageId;
use engine_core::membership::Memberships;
use engine_core::raw::RawMime;
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{
    ConnectionInfo, EmailChunk, EmailStream, ProviderError, ProviderResult, ScopeSync,
};

/// A throwaway directory for one test's data dir.
pub(super) fn temp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "astra-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Writes an account index naming `ids`, the shape the desktop boot keeps.
pub(super) fn write_index(data_dir: &std::path::Path, ids: &[&str]) {
    std::fs::create_dir_all(data_dir).unwrap();
    let text = serde_json::json!({ "accounts": ids });
    std::fs::write(
        data_dir.join(super::accounts::ACCOUNT_INDEX_FILE),
        serde_json::to_string(&text).unwrap(),
    )
    .unwrap();
}

/// A minimal JMAP-shaped fake: a fixed folder tree, a fixed mailbox served as
/// the first email pass's snapshot, raw sources served by provider key for
/// the body read, and draft writes that record what they were asked and
/// succeed the way a real server would.
pub(super) struct FakeProvider {
    /// The account's whole tree, served as the first folder-list snapshot.
    mailboxes: Vec<Mailbox>,
    /// The account's whole mailbox, served on the first email pass.
    messages: Vec<Message>,
    /// Raw RFC 5322 sources by provider key, for the body read.
    sources: HashMap<String, Vec<u8>>,
    /// Every draft store asked of the fake, each with the key it superseded.
    draft_puts: Mutex<Vec<(Draft, Option<ProviderKey>)>>,
    /// Every draft removal asked of the fake.
    draft_deletes: Mutex<Vec<ProviderKey>>,
    /// The next key a draft store hands back.
    next_key: AtomicU64,
}

impl FakeProvider {
    /// A provider holding `mailboxes` and `messages`.
    pub(super) fn with(mailboxes: Vec<Mailbox>, messages: Vec<Message>) -> Self {
        Self {
            mailboxes,
            messages,
            sources: HashMap::new(),
            draft_puts: Mutex::new(Vec::new()),
            draft_deletes: Mutex::new(Vec::new()),
            next_key: AtomicU64::new(1),
        }
    }

    /// [`with`](Self::with) whose messages also carry raw `sources`, each
    /// `(provider key, raw RFC 5322 bytes)`, so a read has a body to decode.
    pub(super) fn with_sources(
        mailboxes: Vec<Mailbox>,
        messages: Vec<Message>,
        sources: Vec<(&str, &str)>,
    ) -> Self {
        Self {
            sources: sources
                .into_iter()
                .map(|(key, raw)| (key.to_owned(), raw.as_bytes().to_vec()))
                .collect(),
            ..Self::with(mailboxes, messages)
        }
    }

    /// Every recorded draft store, each `(draft, replacing)`.
    pub(super) fn draft_puts(&self) -> Vec<(Draft, Option<ProviderKey>)> {
        self.draft_puts.lock().unwrap().clone()
    }

    /// Every recorded draft removal.
    pub(super) fn draft_deletes(&self) -> Vec<ProviderKey> {
        self.draft_deletes.lock().unwrap().clone()
    }
}

/// The observer the seeding pass runs under: it commits and says nothing.
struct Silent;

impl SyncObserver for Silent {
    fn committed(&self, _commit: &SyncCommit<'_>) {}
}

/// Fills `engine`'s store from `provider` the way a first sync would, so the
/// read verbs have rows to answer with.
pub(super) async fn seed(engine: &Engine, provider: &FakeProvider, account: &AccountId) {
    engine
        .sync_mail(
            std::slice::from_ref(provider),
            account,
            StreamTuning::default(),
            &Silent,
        )
        .await;
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
        let state = if cursor.is_some() {
            "email-2"
        } else {
            "email-1"
        };
        let present = self
            .messages
            .iter()
            .map(|message| message.id.key().clone())
            .collect();
        Box::pin(futures::stream::iter(vec![Ok(EmailChunk::reconcile_last(
            self.messages.clone(),
            present,
            Some(self.messages.len()),
            SyncState::new(state),
        ))]))
    }

    async fn fetch_message_source(
        &self,
        _account: &AccountId,
        message: &Message,
    ) -> ProviderResult<RawMime> {
        self.sources
            .get(message.id.key().as_str())
            .cloned()
            .map(RawMime::new)
            .ok_or_else(|| ProviderError::invalid_state("the fake holds no source for that key"))
    }

    async fn put_draft(
        &self,
        _account: &AccountId,
        draft: &Draft,
        replacing: Option<&ProviderKey>,
    ) -> ProviderResult<ProviderKey> {
        self.draft_puts
            .lock()
            .unwrap()
            .push((draft.clone(), replacing.cloned()));
        let key = ProviderKey::new(format!(
            "draft-key-{}",
            self.next_key.fetch_add(1, Ordering::Relaxed)
        ))
        .unwrap();
        Ok(key)
    }

    async fn delete_draft(&self, _account: &AccountId, draft: &ProviderKey) -> ProviderResult<()> {
        self.draft_deletes.lock().unwrap().push(draft.clone());
        Ok(())
    }
}

impl MailboxWrites for FakeProvider {}
impl CalendarWrites for FakeProvider {}

/// The Inbox fixture every seeding tree leads with.
pub(super) fn inbox() -> Mailbox {
    let mut mailbox = Mailbox::new(MailboxId::try_from("inbox").unwrap(), "Inbox");
    mailbox.role = Some(MailboxRole::Inbox);
    mailbox
}

/// A plain top-level folder.
pub(super) fn folder(key: &str, name: &str) -> Mailbox {
    Mailbox::new(MailboxId::try_from(key).unwrap(), name)
}

/// A message `key` filed in `mailbox` about `subject`, from Ada, delivered at
/// `hour` past a fixed base instant, unread.
pub(super) fn mail(key: &str, mailbox: &str, subject: &str, hour: u8) -> Message {
    let mut message = Message::new(
        MessageId::try_from(key).unwrap(),
        Memberships::of_one(MailboxId::try_from(mailbox).unwrap()),
    );
    message.envelope.subject = Some(subject.to_owned());
    message.envelope.from = vec![EmailAddress::named("Ada Lovelace", "ada@example.test")];
    message.envelope.to = vec![EmailAddress::new("me@example.test")];
    message.received_at = Some(UtcDateTime::new(2026, 10, 9, hour, 0, 0).unwrap());
    message
}

/// Marks a message read (the `$seen` keyword).
pub(super) fn read(mut message: Message) -> Message {
    message
        .keywords
        .insert(Keyword::system(SystemKeyword::Seen));
    message
}
