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
    sync::{atomic::AtomicUsize, Arc, Mutex},
};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Draft, EmailAddress, Engine, Mailbox, MailboxEdit, MailboxId,
    Message, ProviderKey, SystemKeyword, UtcDateTime,
};
use engine_provider::{MailEdit, MessageReport, ReportControls, ReportEvidence, ReportVerdicts};

use crate::{Account, App, AppObserver, MailboxConnector, Surface};

// The message fixtures and the row helpers that read a snapshot back: a child
// module so this file stays under the 500-line limit beside the SMTP leg. The
// re-export keeps every test module's `fakes::…` paths unchanged.
mod mail;
// The folder-tree fixtures and the tree-mutating half of the provider: a child
// module for the same limit, holding the key/folder/inbox builders every test
// names and the `MailboxWrites` leg a folder-edit test drives.
mod tree;
// The provider legs: the sync, source, edit, draft, submit, and report
// overrides the dispatch tests drive, split for the same limit. A trait impl
// cannot split across modules, so the whole `Provider` block lives here.
mod provider;
// The AI pass fixtures, its own child for the same limit, pathed `fakes::ai::…`.
pub(crate) mod ai;
pub(super) use mail::{
    delivered_at, flagged, folder_keys, mail, pane_rows, read, row, row_keys, row_keys_from,
    sending_signals, sent, sent_copy, threaded, trash,
};
pub(super) use tree::{folder, inbox, key};

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
    /// The account's whole tree, served as the first folder-list snapshot and
    /// mutated by the tree edits the fake accepts, the way a server's is.
    /// Behind a mutex because the writes move it.
    pub(super) mailboxes: Arc<Mutex<Vec<Mailbox>>>,
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
    /// How many source fetches the provider has served, so a warm pass can
    /// prove it asked for nothing the store already held.
    pub(super) fetches: Arc<AtomicUsize>,
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
    /// Every submit this provider was asked to deliver, in order: the send
    /// log the outbox assertions read.
    pub(super) submits: Arc<Mutex<Vec<Draft>>>,
    /// How the SMTP leg answers each submit, one entry per attempt in order;
    /// the script runs out into plain delivery.
    send_script: Mutex<Vec<ScriptedSend>>,
    /// Draft saves left to refuse as retryable ("no route to host"), so a test can model
    /// an outage the outbox rides out.
    offline_saves: Mutex<u32>,
    /// Whether the adapter can change the account's folder tree (the
    /// capability); `with_tree_writes` is the shape a folder-edit test drives.
    pub(super) tree_writes: bool,
    /// The report capability the adapter answers with, `None` for a provider
    /// that cannot report at all; `with_reports` is the shape a report test
    /// drives.
    pub(super) report_controls: Option<ReportControls>,
    /// Every `report_message` this provider was asked to apply, in order: each
    /// entry carries the verdict (the keyword a server would train on) beside
    /// the filing destination, the log the report assertions read.
    pub(super) reports: Arc<Mutex<Vec<MessageReport>>>,
    /// Every tree edit this provider was asked to apply, in order: the log the
    /// folder-write assertions read.
    pub(super) mailbox_edits: Arc<Mutex<Vec<MailboxEdit>>>,
}

/// How the fake's SMTP leg answers one submit attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptedSend {
    /// Delivered, and the copy filed in Sent.
    Delivered,
    /// The server could not be reached: a retryable failure that never handed
    /// the message over, so a recovery would rightly try again.
    Unreachable,
    /// The server refused it for good: a permanent failure, nothing delivered.
    Refused,
    /// The message was handed over and no answer came back: the one outcome
    /// that must never be retried without asking.
    Silent,
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
            mailboxes: Arc::new(Mutex::new(mailboxes)),
            messages: Arc::new(Mutex::new(messages)),
            updates: Arc::new(Mutex::new(Vec::new())),
            sources: Mutex::new(sources.into_iter().collect()),
            fetches: Arc::new(AtomicUsize::new(0)),
            syncs: Arc::new(AtomicUsize::new(0)),
            edits: Arc::new(Mutex::new(Vec::new())),
            draft_puts: Arc::new(Mutex::new(Vec::new())),
            draft_deletes: Arc::new(Mutex::new(Vec::new())),
            submits: Arc::new(Mutex::new(Vec::new())),
            send_script: Mutex::new(Vec::new()),
            offline_saves: Mutex::new(0),
            tree_writes: false,
            report_controls: None,
            reports: Arc::new(Mutex::new(Vec::new())),
            mailbox_edits: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Marks the adapter as one that can change the account's folder tree,
    /// accepting and applying tree edits the way a writable server does: the
    /// provider shape every folder-write test drives.
    pub(super) fn with_tree_writes(mut self) -> Self {
        self.tree_writes = true;
        self
    }

    /// Marks the adapter as one that can report mail, the JMAP shape: every
    /// verdict over a convention, and a report leg that records the verdict
    /// and the filing destination rather than acting on them, the way the
    /// edit leg records its edits.
    pub(super) fn with_reports(mut self) -> Self {
        self.report_controls = Some(ReportControls {
            verdicts: ReportVerdicts::all(),
            evidence: ReportEvidence::Convention,
        });
        self
    }

    /// Scripts the SMTP leg's answers to the next submits, one entry per
    /// attempt in order; attempts past the script's end are delivered. The
    /// shape the outbox tests model every send outcome with.
    pub(super) fn submitting(self, script: Vec<ScriptedSend>) -> Self {
        *self.send_script.lock().unwrap() = script;
        self
    }

    /// Every recorded submit, in order, for the delivery assertions.
    pub(super) fn submits(&self) -> Arc<Mutex<Vec<Draft>>> {
        Arc::clone(&self.submits)
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

impl CalendarWrites for FakeProvider {}

/// The connector the fakes would dial on demand; one that never connects, the
/// honest answer for fixtures whose every provider is in hand already.
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
