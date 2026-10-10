// SPDX-License-Identifier: GPL-3.0-only
//! The Astra Mail application core: the protocol of the unidirectional loop, and the
//! runtime that drives it.
//!
//! A host observes a [`Surface`] through [`AppObserver`], pulls an immutable snapshot
//! for it, and dispatches an [`Intent`] back; state flows one way and commands the
//! other. The [`App`] owns one [`engine_api::Engine`] shared across every configured
//! [`Account`], and [`boot`] assembles the whole sequence over a host-provided runtime.
//! The crate root holds the runtime's state; the command surface, the projection, the
//! sync paths, the preferences, and the AI rules model with its generation seam each
//! live in their own module.

mod accounts;
mod ai_results;
mod ai_rules;
mod boot;
mod compose_request;
mod dispatch;
mod drafts;
mod list_rows;
mod mail_writes;
mod outbox;
mod preferences;
mod protocol;
mod reading;
mod reference;
mod row_cache;
mod sanitise;
mod snapshot;
mod submit;
mod surfaced;
mod sync;
mod sync_account;
mod sync_progress;
mod window;

// The gated live test against the local Stalwart harness: a sibling file (the crate's
// convention) that skips unless ASTRA_LIVE_HARNESS=1, so an offline `cargo test`
// never dials anything.
#[cfg(test)]
#[path = "tests_live_harness.rs"]
mod tests_live_harness;

// The dispatch-loop test fixtures: one compiled copy of the fake provider and its
// builders, at the crate root so every test module (the dispatch tests, the list
// tests they split off, and the reading tests) includes the same module rather
// than its own.
#[cfg(test)]
mod dispatch_fakes;

use std::{
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex},
    time::Duration,
};

use engine_api::{AccountId, Engine, Provider};
use tokio::sync::RwLock;

use crate::{
    drafts::Drafts,
    preferences::PreferencesState,
    row_cache::{CachedMailRows, CachedRows},
    window::PAGE,
};

pub use accounts::{Account, MailboxConnector};
pub use ai_results::AiPass;
pub use ai_rules::{
    categorise, categorise_prompt, cold_question, generation_prompt, judge_cold, judge_context,
    match_question, match_rules, prompt_to_rules, Action, AiComplete, AiJudge, CategoriseError,
    Category, MailContext, MatchResult, RulesError, CATEGORISE_PROMPT_VERSION, COLD_JUDGE_VERSION,
    MATCH_CONTEXT_BODY_CAP, RULES_MATCH_VERSION, RULES_PROMPT_VERSION,
};
pub use boot::{boot, BootError, SpawnedTask};
pub use dispatch::AppError;
pub use drafts::{DraftResume, DRAFT_AUTOSAVE_IDLE};
pub use preferences::{load_preferences, preferences_path, save_preferences, Preferences};
pub use protocol::{
    AppObserver, ComposeIntent, ComposerBlob, CompositionId, DraftsIntent, FolderIntent, Intent,
    OutboxIntent, Surface, ViewMode,
};
pub use reference::{FolderRef, MessageRef, QueuedRef, RowRef, ThreadRef};
pub use surfaced::Surfaced;

/// The app runtime: owns one [`Engine`] shared across every configured [`Account`],
/// holds the surface and selection state, and notifies an [`AppObserver`] when a
/// surface changes.
///
/// The engine arrives as an [`Arc`] because the host may hold a second handle (a
/// diagnostic or a harness) beside the app's own; every engine call goes through the
/// shared facade either way.
pub struct App<P> {
    /// The shared engine: one store, account-scoped, the only way mail is read or
    /// synced.
    engine: Arc<Engine>,
    /// The configured accounts, each behind an [`Arc`]. Behind an async `RwLock`
    /// because [`App::add_new_account_deferred`] mutates the set at runtime while the
    /// sync and snapshot paths read it. The `Arc` lets a reader clone one account's
    /// handle out under the guard and then **drop the guard before any network
    /// `.await`**: so a long provider round-trip never holds the (write-preferring)
    /// lock and stalls a concurrent add.
    accounts: RwLock<Vec<Arc<Account<P>>>>,
    /// What the mailbox list is showing: the unified view, one account, or one folder
    /// of one account. One value, so a folder key never travels without the account it
    /// belongs to.
    scope: Mutex<Scope>,
    /// The mailbox-list snapshot and its signal, published as one thing.
    mailbox_list: Surfaced<astra_viewmodel::MailboxListSnapshot>,
    /// The reading snapshot and its signal: the one open message's sanitised
    /// body, or `None` when nothing is open. One cell, because SP2c has one
    /// reading pane; the per-reader map is the multi-window slice's to add.
    reading: Surfaced<Option<astra_viewmodel::ReadingSnapshot>>,
    /// The draft-status snapshot and its signal: the composition whose save moved
    /// and how it ended, or `None` when no composition has a save to report.
    /// One cell for the same reason the reading pane has one: SP2d has one
    /// composer, and the per-composition map is the multi-composer slice's to
    /// add.
    draft_status: Surfaced<Option<astra_viewmodel::DraftStatusSnapshot>>,
    /// The compose-request snapshot and its signal: the composition the last
    /// reply or forward asked the host to open, or `None` when no request is
    /// pending. One cell holding the latest request, not a queue: each request
    /// replaces the one before it, because a composer opened on an older
    /// request would answer a message the user has moved past.
    compose_request: Surfaced<Option<astra_viewmodel::ComposeRequestSnapshot>>,
    /// The message the pending compose request was computed from, held beside
    /// the snapshot rather than read back out of it (the snapshot's contract
    /// carries no reference, and no bare key crosses modules): the submit
    /// arms re-resolve it through the reading resolve path to thread the
    /// outgoing reply or forward under the original's `References` chain.
    /// Replaced by each request that publishes, like the snapshot beside it.
    compose_source: Mutex<Option<crate::reference::MessageRef>>,
    /// The sending snapshot and its signal: every account's queued sends, one
    /// list, published on each transition of the queue. See `outbox`.
    sending: Surfaced<astra_viewmodel::SendingSnapshot>,
    /// The sending state beside the engine's queue: the arrival stamps this
    /// session's reads have noted, which the store holds no counterpart of.
    outbox: Mutex<outbox::Outbox>,
    /// The open compositions and their stored-copy facts. See `drafts`.
    drafts: Mutex<Drafts>,
    /// Held for the length of a draft save, so the read of the stored key and the write
    /// of the new one cannot be split by another save of the same composition.
    save_gate: Arc<tokio::sync::Mutex<()>>,
    /// Woken whenever an autosave arm moves, so the driver re-reads the earliest
    /// deadline instead of sleeping past it.
    autosave_armed: tokio::sync::Notify,
    /// How long a composer sits untouched before its draft is saved. The product
    /// default is [`DRAFT_AUTOSAVE_IDLE`]; a seam so tests (and only they) need
    /// not wait real time.
    autosave_idle: Duration,
    /// The message the reading pane is showing, so a refresh knows what to
    /// re-render (or to close, when the message is gone). Held beside the
    /// snapshot rather than read back out of it: what is open and what was last
    /// published are two facts, and only this one drives the republish.
    open_reading: Mutex<Option<crate::reference::MessageRef>>,
    /// The sync-progress snapshot and its signal: what every mail pass reports its
    /// per-account position through, published on change by the
    /// [`SyncObserver`](engine_api::SyncObserver) fold in `sync_progress`.
    sync_progress: sync_progress::SyncProgress,
    /// The loaded preferences and the path they write through: the view mode, the
    /// expansion map, and the selected folder.
    pane: Mutex<PreferencesState>,
    /// The visible mailbox-list window size (rows from the top). Starts at [`PAGE`],
    /// grows by [`PAGE`] on [`Intent::ShowMore`], and resets to [`PAGE`] on any
    /// navigation that changes the list: so a folder switch always opens at the first
    /// page, and scrolling loads more. A background sync preserves it (the host keeps
    /// whatever it had scrolled into view).
    visible_limit: Mutex<usize>,
    /// A monotonic counter bumped by every intent that changes what the list shows.
    /// A rebuild captures it before its store reads and refuses to publish if the
    /// value moved while it was in flight: intents are spawned, so two can race, and
    /// the older one must not win by finishing last.
    generation: AtomicU64,
    /// The shown list's cached load: every account's tree in pane order, so
    /// re-showing the same list costs nothing. `None` after a sync invalidates it,
    /// until the next read reloads it.
    row_cache: Mutex<Option<CachedRows>>,
    /// The shown folder's cached load: the newest rows of the selected folder's account,
    /// so re-projecting the same list costs nothing. `None` after a sync or a write
    /// invalidates it, until the next read reloads it.
    mail_row_cache: Mutex<Option<CachedMailRows>>,
    /// Bumped on every [`invalidate_list_cache`](App::invalidate_list_cache). A row
    /// load captures this before reading the store and refuses to cache its result if
    /// the value moved while the read was in flight: so a slow pre-sync load cannot
    /// land after the post-sync invalidation and resurrect stale rows.
    row_cache_generation: AtomicU64,
    /// The host-injected port that binds one folder of an account: for a folder the
    /// account lists that no provider of it is bound to (created since it connected).
    /// `None` disables the path (the tests, an unpersisted build).
    connector: Option<Box<dyn MailboxConnector<P>>>,
    /// The AI bundle the incoming pass runs over: the judge and completion
    /// seams, the DLP policy, and the shared AI store. `None` (the
    /// default) and the app never runs a pass.
    ai: Option<Arc<AiPass>>,
    /// Woken by each refresh that arms the incoming AI pass, so the driver
    /// future the host runs picks it up off the refresh's own path.
    pass_armed: tokio::sync::Notify,
}

impl<P> core::fmt::Debug for App<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The engine and providers may hold sensitive handles; show nothing of them.
        f.debug_struct("App").finish_non_exhaustive()
    }
}

impl<P: Provider> App<P> {
    /// Builds the app over an opened `engine` and the initial set of `accounts` it
    /// drives (the host adds more later via
    /// [`add_new_account_deferred`](App::add_new_account_deferred)). Starts in the
    /// unified all-inboxes view; the grouping seeds from the persisted preference once
    /// [`with_prefs_path`](Self::with_prefs_path) names the file.
    ///
    /// `connector` is the host's on-demand folder binder; `observer` is the
    /// notification path every [`Surfaced`] publish signals.
    #[must_use]
    pub fn new(
        engine: Arc<Engine>,
        accounts: Vec<Account<P>>,
        connector: Option<Box<dyn MailboxConnector<P>>>,
        observer: Arc<dyn AppObserver>,
    ) -> Self {
        let sync_progress = sync_progress::SyncProgress::new(Arc::clone(&observer));
        Self {
            engine,
            accounts: RwLock::new(accounts.into_iter().map(Arc::new).collect()),
            scope: Mutex::new(Scope::default()),
            mailbox_list: Surfaced::new(Surface::MailboxList, Arc::clone(&observer)),
            reading: Surfaced::new(Surface::Reading, Arc::clone(&observer)),
            draft_status: Surfaced::new(Surface::DraftStatus, Arc::clone(&observer)),
            compose_request: Surfaced::new(Surface::ComposeRequest, Arc::clone(&observer)),
            compose_source: Mutex::new(None),
            sending: Surfaced::new(Surface::Sending, observer),
            outbox: Mutex::new(outbox::Outbox::default()),
            drafts: Mutex::new(Drafts::default()),
            save_gate: Arc::new(tokio::sync::Mutex::new(())),
            autosave_armed: tokio::sync::Notify::new(),
            autosave_idle: DRAFT_AUTOSAVE_IDLE,
            open_reading: Mutex::new(None),
            sync_progress,
            pane: Mutex::new(PreferencesState::new(None)),
            visible_limit: Mutex::new(PAGE),
            generation: AtomicU64::new(0),
            row_cache: Mutex::new(None),
            mail_row_cache: Mutex::new(None),
            row_cache_generation: AtomicU64::new(0),
            connector,
            ai: None,
            pass_armed: tokio::sync::Notify::new(),
        }
    }

    /// Names the preferences file the app persists through. Call it before the first
    /// dispatch: the view mode, the expansion map, and the selected folder load from
    /// here (defaults while unnamed, as an in-memory build wants), and a persisted
    /// selection seeds the scope so a relaunch reopens the folder that was showing.
    #[must_use]
    pub fn with_prefs_path(mut self, path: Option<PathBuf>) -> Self {
        let pane = Mutex::new(PreferencesState::new(path));
        let scope = pane
            .lock()
            .expect("preferences mutex poisoned")
            .selected_folder()
            .map_or_else(Scope::default, Scope::Folder);
        self.pane = pane;
        self.scope = Mutex::new(scope);
        self
    }

    /// Names how long a composer sits untouched before its draft is saved, in place of
    /// the product's [`DRAFT_AUTOSAVE_IDLE`]. Call it before the first dispatch: the
    /// autosave driver reads the interval each time it arms, and the honest uses are a
    /// test shortening it so thirty seconds cost nothing and a host with a stated
    /// product opinion applying it everywhere.
    #[must_use]
    pub fn with_draft_autosave_idle(mut self, idle: Duration) -> Self {
        self.autosave_idle = idle;
        self
    }

    /// Names the AI bundle the incoming pass runs over (the judge and
    /// completion seams, the DLP policy, and the shared AI store), or
    /// `None` to run no pass (the default). Call it before the first
    /// dispatch: the host assembles the bundle over its client and the one
    /// store, and a refresh arms the pass only when the bundle is in place.
    #[must_use]
    pub fn with_ai_pass(mut self, pass: Option<AiPass>) -> Self {
        self.ai = pass.map(Arc::new);
        self
    }
}

/// The mailbox list's scope. An account is always named alongside a folder, so the
/// folder key, unique only within its account, can never be resolved against the wrong
/// one, or against none: separately held, "no account but a folder" is a state the
/// types would permit and nothing renders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Every account, each one's Inbox in view.
    #[default]
    AllInboxes,
    /// One account's whole mailbox.
    Account(AccountId),
    /// One folder of one account.
    Folder(FolderRef),
}

impl Scope {
    /// The scope for a host's account selection: an id, or the unified list for `None`.
    pub(crate) fn for_account(account: Option<AccountId>) -> Self {
        account.map_or(Self::AllInboxes, Self::Account)
    }
}
