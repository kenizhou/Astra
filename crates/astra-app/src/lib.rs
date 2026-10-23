// SPDX-License-Identifier: GPL-3.0-only
//! The Astra Mail application core: the protocol of the unidirectional loop, and the
//! runtime that drives it.
//!
//! A host observes a [`Surface`] through [`AppObserver`], pulls an immutable snapshot
//! for it, and dispatches an [`Intent`] back; state flows one way and commands the
//! other. The [`App`] owns one [`engine_api::Engine`] shared across every configured
//! [`Account`], and [`boot`] assembles the whole sequence over a host-provided runtime.
//! The crate root holds the runtime's state; the command surface, the projection, the
//! sync paths, the preferences, the AI rules model with its generation seam, the AI
//! replies model with its draft pool service, the AI assist services (knowledge,
//! snippets, autocomplete, translate), and the AI documents services (the digest,
//! the report, the split) each live in their own module.

mod accounts;
mod ai_assist;
mod ai_docs;
mod ai_replies;
mod ai_results;
mod ai_rules;
mod attachments;
mod boot;
mod calendar;
mod compose_open;
mod compose_request;
mod crypto;
mod decode;
mod dispatch;
mod drafts;
mod folder_writes;
mod gate;
mod list_rows;
mod mail_writes;
mod outbox;
mod people;
mod preferences;
mod protocol;
mod query;
mod reading;
mod reference;
mod row_cache;
mod sanitise;
mod search;
mod settings;
mod signatures;
mod snapshot;
mod snapshot_rows;
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

// The calendar slice's test fixtures: the calendar-and-mail provider the
// calendar tests drive, at the crate root for the same reason. Each calendar
// module's tests attach to the module itself, the house pattern.
#[cfg(test)]
#[path = "calendar_fakes/mod.rs"]
mod calendar_fakes;

// The people slice's test fixtures: the contacts-leg provider the people
// tests drive, beside the calendar's for the same reason.
#[cfg(test)]
#[path = "people_fakes/mod.rs"]
mod people_fakes;

// The settings slice's test fixtures: the scripted account store the
// settings tests drive over the account-info seam, beside the calendar's
// and people's for the same reason.
#[cfg(test)]
#[path = "settings_fakes/mod.rs"]
mod settings_fakes;

// The eval harness: the scenario fixtures under the repository's evals/
// directory, driven through the same seams the unit tests exercise. The
// scenario vocabulary and the seam-parameter drive are library surface
// (the desktop host's live suite drives the live-mode fixtures over its
// background client); the offline battery and its fixture injection stay
// test-only, riding the dispatch-loop fakes.
pub mod evals;

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
pub use ai_assist::{
    autocomplete, autocomplete_prompt, extract_knowledge, extraction_prompt, knowledge_context,
    ranking_prompt, search_snippets, translate, translate_prompt, ExtractionSummary,
    KnowledgeEntry, KnowledgeKind, KnowledgeView, Snippet, TranslateLanguage, AUTOCOMPLETE_VERSION,
    KNOWLEDGE_BODY_CAP, KNOWLEDGE_CONTEXT_LIMIT, KNOWLEDGE_EXTRACT_LIMIT,
    KNOWLEDGE_EXTRACT_VERSION, SNIPPET_SEARCH_VERSION, TRANSLATE_CACHE_KIND, TRANSLATE_VERSION,
};
pub use ai_docs::{
    digest_prompt, generate_digest, generate_report, report_prompt, side_for, split_map,
    window_bounds, DigestError, DigestView, DocMessage, DocumentError, DocumentView, ReportError,
    SplitEntry, SplitSide, StoredCategories, DIGEST_PROMPT_VERSION, DOCUMENT_BODY_CAP,
    DOCUMENT_HEAD_LIMIT, DOCUMENT_WINDOW_DEFAULT_DAYS, DOCUMENT_WINDOW_MAX_DAYS,
    DOCUMENT_WINDOW_READ_LIMIT, REPORT_PROMPT_VERSION, SPLIT_MAPPING, SPLIT_MAPPING_VERSION,
};
pub use ai_replies::{
    body_hash, chips_for, draft_prompt, generate_reply, style_profile, style_prompt, summarise,
    summarise_prompt, surface_question, Chip, DraftConfidence, DraftContext, DraftPoolEntry,
    DraftPoolSink, SummariseError, SummariseMessageError, CHIP_CONFIDENCE_FLOOR, CHIP_LIMIT,
    CHIP_PREVIEW_CAP, DRAFT_REPLY_PROMPT_VERSION, GATE_JUDGE_VERSION, STYLE_PROFILE_MAX_AGE_SECS,
    STYLE_PROMPT_VERSION, STYLE_SAMPLE_BODY_CAP, STYLE_SAMPLE_LIMIT, STYLE_SENT_WINDOW,
    SUMMARISE_PROMPT_VERSION, THREAD_BODY_CAP, THREAD_HISTORY_LIMIT,
};
pub use ai_results::AiPass;
pub use ai_rules::{
    categorise, categorise_prompt, cold_question, generation_prompt, judge_cold, judge_context,
    match_question, match_rules, prompt_to_rules, Action, AiComplete, AiJudge, CategoriseError,
    Category, MailContext, MatchResult, RulesError, CATEGORISE_PROMPT_VERSION, COLD_JUDGE_VERSION,
    MATCH_CONTEXT_BODY_CAP, RULES_MATCH_VERSION, RULES_PROMPT_VERSION,
};
pub use attachments::{AttachmentContent, AttachmentReadError};
pub use boot::{boot, BootError, SpawnedTask};
pub use calendar::{
    generate_meeting_brief, meeting_brief_prompt, BriefDocumentError, BriefMeeting,
    BriefMeetingRow, MeetingBriefError, MeetingBriefState, MeetingBriefView,
    BRIEF_INVITATION_SCAN_LIMIT, MEETING_BRIEF_PROMPT_VERSION,
};
pub use compose_request::OpenDraftError;
pub use crypto::CryptoFlags;
pub use decode::{DecodeOutcome, MessageDecoder};
pub use dispatch::AppError;
pub use drafts::{DraftResume, DRAFT_AUTOSAVE_IDLE};
pub use folder_writes::FolderWriteError;
pub use gate::{BeforeSendGate, ComposeTransform, GateComposition, GateVerdict, TransformFailure};
pub use mail_writes::MailWriteError;
pub use people::avatar::AvatarView;
pub use preferences::{load_preferences, preferences_path, save_preferences, Preferences};
pub use protocol::{
    AppObserver, CalendarIntent, ComposeIntent, ComposerBlob, CompositionId, ContactDraftInput,
    ContactNameInput, ContactPatchInput, DavSectionInput, DraftsIntent, EventDrag, EventEdge,
    EventEdit, FolderIntent, Intent, OutboxIntent, PeopleIntent, RecurrenceChange, RepeatEnd,
    RepeatRule, RsvpAnswer, SearchScope, SettingsIntent, Surface, ThemeMode, ViewMode, WeekdayWire,
};
pub use query::{MessageDetail, MessagePage, QueryAccount, QueryRow, MAX_PAGE};
pub use reference::{
    AddressBookRef, CalendarRef, CardRef, EventRef, FolderRef, MessageRef, QueuedRef, RowRef,
    ThreadRef,
};
pub use settings::{AccountInfo, DavLegs, HostAccountOps};
pub use signatures::{seed_signature, swap_seeded, AccountSignatures};
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
    /// The original's part ids the pending compose request's forward is
    /// holding onto, set beside `compose_source` by the request that
    /// published and read by the forward's submit to resolve the carried
    /// rows' bytes off the original. Empty for every other kind.
    compose_carry: Mutex<Vec<u32>>,
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
    /// The calendar snapshot and its signal: the calendar manager set, the
    /// visible window's occurrences, the write states, and the now line, as
    /// one thing. See `calendar`.
    calendar: Surfaced<astra_viewmodel::CalendarSnapshot>,
    /// The calendar view state a rebuild projects and the intents move: the
    /// window, the visible set, the anchor, the horizon watermark, the write
    /// states, and the reply prompts. See `calendar`.
    calendar_state: Mutex<calendar::CalendarState>,
    /// The people snapshot and its signal: the unified rows, the open
    /// person's detail, the write states, and the create targets, as one
    /// thing. See `people`.
    people: Surfaced<astra_viewmodel::PeopleSnapshot>,
    /// The people view state a rebuild projects and the intents move: the
    /// contacts legs the host installed, the current query, the open person,
    /// and the write states. See `people`.
    people_state: Mutex<people::PeopleState>,
    /// The settings snapshot and its signal: the persisted settings block,
    /// the account rows, and the about block, as one thing. See `settings`.
    settings: Surfaced<astra_viewmodel::SettingsSnapshot>,
    /// The settings view state a rebuild projects and the intents move: the
    /// per-account dav failures and the newest write's refusal. See
    /// `settings`.
    settings_state: Mutex<settings::SettingsState>,
    /// The host-installed account store: the seam the settings surface reads
    /// the account configs through and writes them back over. `None` (the
    /// default) until the host installs one; see `settings`.
    account_ops: Mutex<Option<Arc<dyn settings::HostAccountOps<P>>>>,
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
    /// The live search session: the query the field holds and the scope it
    /// runs under, or no query at all. Session state, never persisted; see
    /// `search`.
    search: Mutex<crate::search::SearchSession>,
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
    /// The reply drafts an open has armed and the driver has not taken
    /// yet: one entry per message, the open's reading correlation riding
    /// each. See `ai_replies::drive`.
    reply_arms: Mutex<Vec<crate::ai_replies::DraftArm>>,
    /// Woken by each open that arms a reply draft, so the driver future
    /// the host runs picks the arming up off the open's own path.
    reply_armed: tokio::sync::Notify,
    /// The plugin send gate: every submit checks with it before queueing.
    /// `None` (the default) and a submit costs nothing.
    pub(crate) send_gate: Option<Arc<dyn crate::gate::BeforeSendGate>>,
    /// The plugin compose transform: every submit's rendered legs pass it
    /// before the gate. `None` (the default) sends what the composer
    /// rendered.
    pub(crate) compose_transform: Option<Arc<dyn crate::gate::ComposeTransform>>,
    /// The plugin message decoder: every reading open's raw source passes it
    /// before the render. `None` (the default) renders the store's own read.
    pub(crate) decoder: Option<Arc<dyn crate::decode::MessageDecoder>>,
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
            compose_carry: Mutex::new(Vec::new()),
            calendar: Surfaced::new(Surface::Calendar, Arc::clone(&observer)),
            calendar_state: Mutex::new(calendar::CalendarState::new()),
            people: Surfaced::new(Surface::People, Arc::clone(&observer)),
            people_state: Mutex::new(people::PeopleState::new()),
            settings: Surfaced::new(Surface::Settings, Arc::clone(&observer)),
            settings_state: Mutex::new(settings::SettingsState::default()),
            account_ops: Mutex::new(None),
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
            search: Mutex::new(crate::search::SearchSession::default()),
            mail_row_cache: Mutex::new(None),
            row_cache_generation: AtomicU64::new(0),
            connector,
            ai: None,
            pass_armed: tokio::sync::Notify::new(),
            reply_arms: Mutex::new(Vec::new()),
            reply_armed: tokio::sync::Notify::new(),
            send_gate: None,
            compose_transform: None,
            decoder: None,
        }
    }

    /// Names the preferences file the app persists through. Call it before the first
    /// dispatch: the view mode, the expansion map, and the selected scope load from
    /// here (defaults while unnamed, as an in-memory build wants), and a persisted
    /// selection seeds the scope so a relaunch reopens the folder, the account, or
    /// the unified view that was showing.
    #[must_use]
    pub fn with_prefs_path(mut self, path: Option<PathBuf>) -> Self {
        let pane = Mutex::new(PreferencesState::new(path));
        let scope = {
            let pane = pane.lock().expect("preferences mutex poisoned");
            pane.selected_folder().map_or_else(
                || {
                    pane.selected_account()
                        .map_or_else(Scope::default, Scope::Account)
                },
                Scope::Folder,
            )
        };
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
    // The plugin ports' builders (`with_send_gate`, `with_decoder`) live in
    // their own modules, beside the ports they install.
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

/// A unique-per-intent idempotency key under `prefix`, from the wall clock
/// and a process counter: the one mint every write family shares, so no
/// family carries its own copy of the recipe.
pub(crate) fn mint_idempotency(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("{prefix}:{nanos}:{seq}")
}
