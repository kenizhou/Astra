// SPDX-License-Identifier: GPL-3.0-only
//! The shell's managed state: the app core the command surface drives, the runtime
//! it drives on, the per-account dial table, and the credential store. One value,
//! registered once at boot, read back by every command through Tauri's `State`.

use std::sync::Arc;

use astra_ai::{AiDocumentKind, KnowledgeId};
use astra_app::{
    Account, App, AvatarView, BriefDocumentError, CardRef, Chip, CompositionId, ContactDraftInput,
    DigestView, DocumentError, DocumentView, DraftResume, ExtractionSummary, Intent, KnowledgeView,
    MeetingBriefView, Snippet, SplitEntry, SummariseMessageError, TranslateLanguage,
};
use astra_viewmodel::{
    CalendarSnapshot, ComposeRequestSnapshot, DraftStatusSnapshot, MailboxListSnapshot,
    PeopleSnapshot, ReadingSnapshot, SendingSnapshot, SettingsSnapshot, SyncProgressSnapshot,
};
use async_trait::async_trait;
use engine_api::{AccountId, CalendarDate, Provider, ProviderKey, RecipientSuggestion};

use crate::mcp::McpHost;
use crate::{AccountCredentialStore, AccountDials, AppRuntime, LlmClient, SidecarRelay};

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

    /// The published compose-request snapshot, for the host's pull: the
    /// composition the last reply or forward opened, or `None` when no
    /// request is pending.
    fn compose_request(&self) -> Option<ComposeRequestSnapshot>;

    /// Takes the pending compose request, consuming the slot: the pop-out
    /// composer window's pull, so a later window never re-reads a spent
    /// request.
    fn take_compose_request(&self) -> Option<ComposeRequestSnapshot>;

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

    /// The message's smart-reply chips: the reading pane's pull on open and
    /// on the draft-pool event, answered through the surfacing gate.
    async fn draft_chips(&self, account: &AccountId, key: &ProviderKey) -> Vec<Chip>;

    /// Summarises the message, cached: a hit answers without a model call.
    ///
    /// # Errors
    ///
    /// Returns the core's own failure when no AI bundle runs, the message
    /// cannot be resolved, or the action itself fails.
    async fn summarise_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<String, SummariseMessageError>;

    /// The account's writing-style note, or `None` when none was built.
    async fn account_style_note(&self, account: &AccountId) -> Option<String>;

    /// Records the reader's dismissal of the message's chips.
    fn dismiss_chips(&self, account: &AccountId, key: &ProviderKey);

    /// Publishes the reply composition one accepted chip opens: the reply's
    /// own prefill with the drafted reply already the document's opening
    /// paragraphs, signalled through the compose-request surface.
    async fn compose_from_draft(&self, account: &AccountId, key: &ProviderKey, draft: &str);

    /// Every knowledge entry the account holds, newest first, as the
    /// knowledge surface lists it. An app with no AI bundle answers
    /// empty.
    fn knowledge_list(&self, account: &AccountId) -> Vec<KnowledgeView>;

    /// Runs one knowledge extraction over the account's most recent
    /// mail, `limit` messages at most, answering the run's summary. An
    /// app with no AI bundle answers the empty run.
    async fn knowledge_extract(&self, account: &AccountId, limit: usize) -> ExtractionSummary;

    /// Forgets one knowledge entry, answering whether any row went.
    fn knowledge_forget(&self, id: &KnowledgeId) -> bool;

    /// Every snippet, the most recently touched first: the picker's
    /// plain listing read.
    fn snippet_list(&self) -> Vec<Snippet>;

    /// Searches the snippet library, the ranked answer with its
    /// substring fallback.
    async fn snippet_search(&self, query: &str) -> Vec<Snippet>;

    /// The compose autocomplete suggestion for the editor's text around
    /// the cursor; an empty answer never shows a chip.
    async fn compose_autocomplete(&self, before: &str, after: &str) -> String;

    /// Translates the message's body into `lang`, the answer served
    /// through the text cache; an empty answer names a message that
    /// cannot be resolved or a call that failed.
    async fn translate_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        lang: TranslateLanguage,
    ) -> String;

    /// Generates the account's digest over the last `days` days and
    /// stores it under the window's period.
    ///
    /// # Errors
    ///
    /// Returns the core's own failure when no AI bundle runs, the
    /// window cannot be read, the generation fails, or the store
    /// refuses the put.
    async fn generate_digest(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError>;

    /// Generates the account's analytics report over the last `days`
    /// days and stores it under the window's period.
    ///
    /// # Errors
    ///
    /// Returns the core's own failure when no AI bundle runs, the
    /// window cannot be read, the generation fails, or the store
    /// refuses the put.
    async fn generate_report(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError>;

    /// The account's newest stored document of one kind, or `None`
    /// when no generation has stored one.
    fn latest_document(&self, account: &AccountId, kind: AiDocumentKind) -> Option<DocumentView>;

    /// The account's split map over its stored categorisation: the
    /// pure derivation, no model call at all.
    async fn split_map(&self, account: &AccountId) -> Vec<SplitEntry>;

    /// The published calendar snapshot, for the host's pull after a
    /// calendar signal.
    fn calendar(&self) -> CalendarSnapshot;

    /// The published people snapshot, for the host's pull after a
    /// people signal.
    fn people(&self) -> PeopleSnapshot;

    /// The published settings snapshot, for the host's pull after a
    /// settings signal.
    fn settings(&self) -> SettingsSnapshot;

    /// The editor's seed for one card: the stored card the reference names,
    /// rendered as the editor's input subset, or `None` when nobody stores
    /// it.
    async fn card_seed(&self, card: &CardRef) -> Option<ContactDraftInput>;

    /// The ranked recipient suggestions for one completion token: the
    /// composer's autosuggest pull, providerless.
    async fn recipient_suggestions(&self, token: &str, limit: usize) -> Vec<RecipientSuggestion>;

    /// The avatar one address resolves to: the row's own monogram
    /// derivation and the cached photo file's path.
    async fn avatar_by_email(&self, email: &str) -> AvatarView;

    /// Generates the account's meeting brief over the civil day: the
    /// store re-read of the day's occurrences and one generation, the
    /// empty day answered as the no-meetings view with no call spent.
    ///
    /// # Errors
    ///
    /// Returns the core's own failure when no AI bundle runs, the day
    /// cannot be read, or the generation fails.
    async fn generate_meeting_brief(
        &self,
        account: &AccountId,
        day: CalendarDate,
    ) -> Result<MeetingBriefView, BriefDocumentError>;
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

    fn compose_request(&self) -> Option<ComposeRequestSnapshot> {
        App::compose_request(self)
    }

    fn take_compose_request(&self) -> Option<ComposeRequestSnapshot> {
        App::take_compose_request(self)
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

    async fn draft_chips(&self, account: &AccountId, key: &ProviderKey) -> Vec<Chip> {
        App::draft_chips(self, account, key).await
    }

    async fn summarise_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<String, SummariseMessageError> {
        App::summarise_message(self, account, key).await
    }

    async fn account_style_note(&self, account: &AccountId) -> Option<String> {
        App::account_style_note(self, account).await
    }

    fn dismiss_chips(&self, account: &AccountId, key: &ProviderKey) {
        App::dismiss_chips(self, account, key);
    }

    async fn compose_from_draft(&self, account: &AccountId, key: &ProviderKey, draft: &str) {
        App::compose_from_draft(self, account, key, draft).await;
    }

    fn knowledge_list(&self, account: &AccountId) -> Vec<KnowledgeView> {
        App::knowledge_list(self, account)
    }

    async fn knowledge_extract(&self, account: &AccountId, limit: usize) -> ExtractionSummary {
        App::knowledge_extract(self, account, limit).await
    }

    fn knowledge_forget(&self, id: &KnowledgeId) -> bool {
        App::knowledge_forget(self, id)
    }

    fn snippet_list(&self) -> Vec<Snippet> {
        App::snippet_list(self)
    }

    async fn snippet_search(&self, query: &str) -> Vec<Snippet> {
        App::snippet_search(self, query).await
    }

    async fn compose_autocomplete(&self, before: &str, after: &str) -> String {
        App::compose_autocomplete(self, before, after).await
    }

    async fn translate_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        lang: TranslateLanguage,
    ) -> String {
        App::translate_message(self, account, key, lang).await
    }

    async fn generate_digest(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError> {
        App::generate_digest(self, account, days).await
    }

    async fn generate_report(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError> {
        App::generate_report(self, account, days).await
    }

    fn latest_document(&self, account: &AccountId, kind: AiDocumentKind) -> Option<DocumentView> {
        App::latest_document(self, account, kind)
    }

    async fn split_map(&self, account: &AccountId) -> Vec<SplitEntry> {
        App::split_map(self, account).await
    }

    fn calendar(&self) -> CalendarSnapshot {
        App::calendar(self)
    }

    fn people(&self) -> PeopleSnapshot {
        App::people(self)
    }

    fn settings(&self) -> SettingsSnapshot {
        App::settings(self)
    }

    async fn card_seed(&self, card: &CardRef) -> Option<ContactDraftInput> {
        App::card_seed(self, card).await
    }

    async fn recipient_suggestions(&self, token: &str, limit: usize) -> Vec<RecipientSuggestion> {
        App::recipient_suggestions(self, token, limit).await
    }

    async fn avatar_by_email(&self, email: &str) -> AvatarView {
        App::avatar_by_email(self, email).await
    }

    async fn generate_meeting_brief(
        &self,
        account: &AccountId,
        day: CalendarDate,
    ) -> Result<MeetingBriefView, BriefDocumentError> {
        App::generate_meeting_brief(self, account, day).await
    }
}

/// The state the shell manages: everything a command needs, in one registered value.
///
/// The field order is the drop order, and it is load bearing: the agent
/// relay must drop before the runtime, because the runtime's shutdown
/// waits for the relay's read pump and the pump waits on the sidecar
/// child, which waits on the relay's stdin closing.
pub(crate) struct AppState {
    /// The app core behind the command surface.
    pub(crate) app: Arc<dyn AppPort>,
    /// The agent sidecar relay: prompts, approvals, and the ledger the
    /// sidecar's calls land in.
    pub(crate) agent: Arc<SidecarRelay>,
    /// The MCP listener host: the advanced panel's run/stop seam and
    /// account gate. It carries a runtime of its own, so its drop
    /// carries no ordering constraint against the app runtime below.
    pub(crate) mcp: Arc<McpHost>,
    /// The background LLM client, when a tier table resolved at boot: the
    /// use-case chains, the provider keys, and the ledger its attempts
    /// land in.
    pub(crate) llm: Option<Arc<LlmClient>>,
    /// Every registered account's config and connection pool: what a re-dial, a
    /// folder bind, and the boot's restore read.
    pub(crate) connections: Arc<AccountDials>,
    /// The engine's contact-photo area, beside the store database: the one
    /// directory the photo command serves, so the webview never gains a
    /// protocol wide enough to read host files itself.
    pub(crate) photo_root: std::path::PathBuf,
    /// The OS secure store the account-add flow persists through.
    pub(crate) credential_store: Arc<dyn AccountCredentialStore>,
    /// The OAuth sign-in surface: the session over the system browser and
    /// the deep-link capture, beside the registered provider clients. The
    /// wizard's sign-in commands drive it; a build carrying no
    /// registration offers nothing.
    pub(crate) oauth: Arc<crate::oauth::commands::SignInSurface>,
    /// The standing grant refusals: what the boot dial and the sign-in
    /// completions learn about every Microsoft account's grant, read by
    /// the host account store the settings rows render through.
    pub(crate) refusals: Arc<crate::oauth::grant_refusals::GrantRefusals>,
    /// The app's async runtime: dispatches, first syncs and blocking dials run on it.
    pub(crate) runtime: AppRuntime,
}

impl core::fmt::Debug for AppState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The app core and the pools may hold sensitive handles; count, do not show.
        f.debug_struct("AppState")
            .field("accounts", &self.connections.ids().len())
            .field("agent", &self.agent.status().as_str())
            .field("llm", &self.llm.is_some())
            .finish_non_exhaustive()
    }
}
