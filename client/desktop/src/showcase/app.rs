// SPDX-License-Identifier: GPL-3.0-only
//! The showcase's fixture app: an [`AppPort`](crate::app_state::AppPort)
//! answering the recorded snapshots for the named screen and an honest
//! empty for every other pull. The recorded answers are the web fixture
//! harness's own JSON, embedded at compile time from the one directory
//! they live in, so the production webview bundle carries nothing new and
//! the fixtures keep a single source of truth.
//!
//! The app is static by design: a dispatch settles and changes nothing,
//! no surface is signalled to re-pull, and the screens stand as recorded,
//! their dates frozen with them. A screenshot run needs states, not
//! transitions.

#[cfg(test)]
use std::sync::Arc;

use astra_ai::{AiDocumentKind, KnowledgeId};
use astra_app::{
    AnalyticsEvent, AppError, AttachmentContent, AttachmentReadError, AvatarView,
    BriefDocumentError, CardRef, Chip, CompositionId, ContactDraftInput, DigestView, DocumentError,
    DocumentView, DraftResume, EmlContent, EmlReadError, ExtractionSummary, Intent, KnowledgeView,
    MeetingBriefView, NewMailBatch, PassOutcome, Snippet, SplitEntry, SummariseMessageError,
    TranslateLanguage,
};
use astra_viewmodel::{
    CalendarSnapshot, ComposeRequestSnapshot, DraftStatusSnapshot, EventDetail,
    MailboxListSnapshot, MailtoOpen, PeopleSnapshot, ReadingSnapshot, SendingSnapshot,
    SettingsSnapshot, SyncProgressSnapshot,
};
use engine_api::{AccountId, CalendarDate, ProviderKey, RecipientSuggestion};

use crate::app_state::{AppPort, HostAccount};

use super::Screen;

// The recorded answers, embedded from the fixture harness's directory at
// compile time: the same JSON the browser harness serves, parsed here
// into the snapshots the command layer hands the webview.
const MAILBOX_LIST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/src/services/__fixtures__/mailbox_list.json"
));
const READING_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/src/services/__fixtures__/reading_snapshot.json"
));
const SETTINGS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/src/services/__fixtures__/settings_snapshot.json"
));
const PEOPLE_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/src/services/__fixtures__/people_snapshot.json"
));

/// Reads one recorded fixture as the snapshot type that serves it.
fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|err| format!("a showcase fixture could not be read: {err}"))
}

/// The fixture app: the recorded answers, parsed once at build.
pub(crate) struct ShowcaseApp {
    /// The recorded mailbox list: the shell's spine, answered for every
    /// screen, because an empty account tree would open the account-setup
    /// flow over every screenshot.
    mailbox_list: MailboxListSnapshot,
    /// The recorded reading snapshot, standing open for the reading
    /// screen alone.
    reading: Option<ReadingSnapshot>,
    /// The recorded settings snapshot, answered for the settings screen
    /// alone.
    settings: Option<SettingsSnapshot>,
    /// The recorded people snapshot, answered for the people screen alone.
    people: Option<PeopleSnapshot>,
}

impl ShowcaseApp {
    /// The app for one screen: the mailbox list always answers, the named
    /// screen's family beside it, everything else at its empty.
    pub(crate) fn for_screen(screen: Screen) -> Result<Self, String> {
        Ok(Self {
            mailbox_list: parse(MAILBOX_LIST_JSON)?,
            reading: (screen == Screen::Reading)
                .then(|| parse(READING_JSON))
                .transpose()?,
            settings: (screen == Screen::Settings)
                .then(|| parse(SETTINGS_JSON))
                .transpose()?,
            people: (screen == Screen::People)
                .then(|| parse(PEOPLE_JSON))
                .transpose()?,
        })
    }
}

/// The fixture app behind the seam the command surface holds, for the
/// tests; the boot's assembly builds the app it shares with the MCP
/// backend directly.
#[cfg(test)]
pub(crate) fn fixture_app(screen: Screen) -> Arc<dyn AppPort> {
    Arc::new(ShowcaseApp::for_screen(screen).expect("the compiled-in fixtures parse"))
}

#[async_trait::async_trait]
impl AppPort for ShowcaseApp {
    async fn dispatch(&self, _intent: Intent) -> Result<(), AppError> {
        // A showcase dispatch settles and changes nothing: the screens
        // stand as recorded, and no surface is signalled to re-pull.
        Ok(())
    }

    fn mailbox_list(&self) -> MailboxListSnapshot {
        self.mailbox_list.clone()
    }

    fn sync_progress(&self) -> SyncProgressSnapshot {
        SyncProgressSnapshot::default()
    }

    fn reading(&self) -> Option<ReadingSnapshot> {
        self.reading.clone()
    }

    async fn attachment_content(
        &self,
        _account: &AccountId,
        _key: &ProviderKey,
        _id: u32,
    ) -> Result<Option<AttachmentContent>, AttachmentReadError> {
        Err(AttachmentReadError::Unresolved)
    }

    async fn eml_content(
        &self,
        _account: &AccountId,
        _key: &ProviderKey,
    ) -> Result<EmlContent, EmlReadError> {
        Err(EmlReadError::Unresolved)
    }

    fn draft_status(&self) -> Option<DraftStatusSnapshot> {
        None
    }

    fn compose_request(&self) -> Option<ComposeRequestSnapshot> {
        None
    }

    fn take_compose_request(&self) -> Option<ComposeRequestSnapshot> {
        None
    }

    async fn open_mailto(&self, _uri: &str) -> Option<MailtoOpen> {
        None
    }

    fn sending(&self) -> SendingSnapshot {
        SendingSnapshot::default()
    }

    async fn resume_latest_draft(
        &self,
        _account: &AccountId,
        _composition: CompositionId,
    ) -> Option<DraftResume> {
        None
    }

    async fn resume_draft_by_key(
        &self,
        _message: &astra_app::MessageRef,
        _composition: CompositionId,
    ) -> Option<DraftResume> {
        None
    }

    async fn add_new_account_deferred(&self, _account: HostAccount) {}

    async fn sync_added_account(&self, _id: &AccountId) {}

    async fn refresh_calendars_if_changed(&self) {}

    async fn draft_chips(&self, _account: &AccountId, _key: &ProviderKey) -> Vec<Chip> {
        Vec::new()
    }

    async fn summarise_message(
        &self,
        _account: &AccountId,
        _key: &ProviderKey,
    ) -> Result<String, SummariseMessageError> {
        Err(SummariseMessageError::Unresolved)
    }

    async fn account_style_note(&self, _account: &AccountId) -> Option<String> {
        None
    }

    fn dismiss_chips(&self, _account: &AccountId, _key: &ProviderKey) {}

    async fn compose_from_draft(&self, _account: &AccountId, _key: &ProviderKey, _draft: &str) {}

    fn knowledge_list(&self, _account: &AccountId) -> Vec<KnowledgeView> {
        Vec::new()
    }

    async fn knowledge_extract(&self, _account: &AccountId, _limit: usize) -> ExtractionSummary {
        ExtractionSummary::default()
    }

    fn knowledge_forget(&self, _id: &KnowledgeId) -> bool {
        false
    }

    fn snippet_list(&self) -> Vec<Snippet> {
        Vec::new()
    }

    async fn snippet_search(&self, _query: &str) -> Vec<Snippet> {
        Vec::new()
    }

    async fn compose_autocomplete(&self, _before: &str, _after: &str) -> String {
        String::new()
    }

    async fn translate_message(
        &self,
        _account: &AccountId,
        _key: &ProviderKey,
        _lang: TranslateLanguage,
    ) -> String {
        String::new()
    }

    async fn generate_digest(
        &self,
        _account: &AccountId,
        _days: u32,
    ) -> Result<DigestView, DocumentError> {
        Err(DocumentError::NoBundle)
    }

    async fn generate_report(
        &self,
        _account: &AccountId,
        _days: u32,
    ) -> Result<DigestView, DocumentError> {
        Err(DocumentError::NoBundle)
    }

    fn latest_document(&self, _account: &AccountId, _kind: AiDocumentKind) -> Option<DocumentView> {
        None
    }

    async fn split_map(&self, _account: &AccountId) -> Vec<SplitEntry> {
        Vec::new()
    }

    fn calendar(&self) -> CalendarSnapshot {
        CalendarSnapshot::default()
    }

    async fn calendar_event_detail(&self, _event: &astra_app::EventRef) -> Option<EventDetail> {
        None
    }

    fn people(&self) -> PeopleSnapshot {
        self.people.clone().unwrap_or_default()
    }

    fn settings(&self) -> SettingsSnapshot {
        self.settings.clone().unwrap_or_default()
    }

    fn record_analytics(&self, _event: &AnalyticsEvent) {}

    async fn card_seed(&self, _card: &CardRef) -> Option<ContactDraftInput> {
        None
    }

    async fn recipient_suggestions(&self, _token: &str, _limit: usize) -> Vec<RecipientSuggestion> {
        Vec::new()
    }

    async fn avatar_by_email(&self, _email: &str) -> AvatarView {
        AvatarView::default()
    }

    async fn generate_meeting_brief(
        &self,
        _account: &AccountId,
        _day: CalendarDate,
    ) -> Result<MeetingBriefView, BriefDocumentError> {
        Err(BriefDocumentError::NoBundle)
    }

    fn drain_new_mail(&self) -> NewMailBatch {
        NewMailBatch::default()
    }

    fn record_pass_outcome(&self, _account: &AccountId, _outcome: PassOutcome) {}

    async fn wait_first_pass(&self, _account: &AccountId) {}
}
