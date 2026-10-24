// SPDX-License-Identifier: GPL-3.0-only
//! The fake app's [`AppPort`] impl and the A3 assist pulls' seeds, split
//! out of the fixture root so that root stays under the 500-line limit:
//! one trait impl for the one fake, the inherited contract beside the
//! assist answers, everything the command tests seed or assert in one
//! place behind the fake's own locks.

use astra_ai::{AiDocumentKind, KnowledgeId};
use astra_app::{
    AppError, AttachmentContent, AttachmentReadError, AvatarView, BriefDocumentError, CardRef,
    Chip, CompositionId, ContactDraftInput, DigestView, DocumentError, DocumentView, DraftResume,
    ExtractionSummary, Intent, KnowledgeView, MeetingBriefView, NewMailBatch, PassOutcome, Snippet,
    SplitEntry, SummariseMessageError, TranslateLanguage,
};
use astra_viewmodel::{
    CalendarSnapshot, ComposeKind, ComposeRequestSnapshot, DraftStatusSnapshot,
    MailboxListSnapshot, MailtoOpen, PeopleSnapshot, ReadingSnapshot, SendingSnapshot,
    SettingsSnapshot, SyncProgressSnapshot,
};
use engine_api::{AccountId, CalendarDate, ProviderKey, RecipientSuggestion};

use crate::app_state::{AppPort, HostAccount};

use super::{compose_prefill, open_snapshot, FakeApp};

/// The assist pulls' seeds and their record: every field the A3 command
/// tests seed or assert, in one place.
#[derive(Default)]
pub(crate) struct AssistSeeds {
    /// The knowledge listing's answer, the rows the fake holds.
    pub(crate) knowledge: Vec<KnowledgeView>,
    /// The accounts the knowledge listing was asked about.
    pub(crate) knowledge_into: Vec<String>,
    /// The extraction pull's answer.
    pub(crate) extraction: ExtractionSummary,
    /// The (account, limit) pairs the extraction pull was asked for.
    pub(crate) extracted_into: Vec<(String, usize)>,
    /// The ids the forget was asked to remove.
    pub(crate) forgotten_into: Vec<String>,
    /// The snippet listing's and the search's answer.
    pub(crate) snippets: Vec<Snippet>,
    /// The search texts the search pull was asked for.
    pub(crate) searched_into: Vec<String>,
    /// The autocomplete pull's answer.
    pub(crate) suggestion: String,
    /// The (before, after) pairs the autocomplete pull was asked for.
    pub(crate) suggested_into: Vec<(String, String)>,
    /// The translate action's answer.
    pub(crate) translation: String,
    /// The (account, key, lang) triples the translate action was asked
    /// for, the lang as its wire word.
    pub(crate) translated_into: Vec<(String, String, String)>,
}

/// The A4 documents pulls' seeds and their record: every field the
/// documents command tests seed or assert, in one place.
pub(crate) struct DocsSeeds {
    /// The generate pulls' answer, an `Err` standing in for the typed
    /// refusal a test maps onto the command error.
    pub(crate) generated: Result<DigestView, DocumentError>,
    /// The (account, kind word, days) triples the generate pulls were
    /// asked for, the days as the boundary clamped them.
    pub(crate) generated_into: Vec<(String, String, u32)>,
    /// The latest read's answer.
    pub(crate) latest: Option<DocumentView>,
    /// The (account, kind word) pairs the latest read was asked for.
    pub(crate) latest_into: Vec<(String, String)>,
    /// The split pull's answer.
    pub(crate) split: Vec<SplitEntry>,
    /// The accounts the split pull was asked about.
    pub(crate) split_into: Vec<String>,
}

impl Default for DocsSeeds {
    fn default() -> Self {
        Self {
            generated: Ok(DigestView {
                period_start: 0,
                period_end: 0,
                content: String::new(),
                regenerated: false,
            }),
            generated_into: Vec::new(),
            latest: None,
            latest_into: Vec::new(),
            split: Vec::new(),
            split_into: Vec::new(),
        }
    }
}

/// The calendar brief pull's seeds and their record: the answer (an
/// `Err` standing in for the typed refusal a test maps onto the command
/// error) and the (account, day) pairs the pull was asked for, the day
/// as its wire word.
pub(crate) struct BriefSeeds {
    /// The brief pull's answer.
    pub(crate) brief: Result<MeetingBriefView, BriefDocumentError>,
    /// The (account, day word) pairs the brief pull was asked for.
    pub(crate) brief_into: Vec<(String, String)>,
}

impl Default for BriefSeeds {
    fn default() -> Self {
        Self {
            brief: Ok(MeetingBriefView {
                account: AccountId::try_from("work").expect("an account id"),
                day: CalendarDate::new(1970, 1, 1).expect("the epoch day"),
                state: astra_app::MeetingBriefState::NoMeetings,
                brief: String::new(),
                meetings: Vec::new(),
            }),
            brief_into: Vec::new(),
        }
    }
}

impl FakeApp {
    /// Seeds the fake's knowledge listing with one entry, minted around
    /// `id`, so the listing and the forget have a row to round trip.
    pub(crate) fn holding_knowledge(&self, id: &str, kind: &str, text: &str) {
        self.assist.lock().unwrap().knowledge.push(KnowledgeView {
            id: KnowledgeId::new(id.to_owned()),
            kind: kind.to_owned(),
            text: text.to_owned(),
            source_account: "work".to_owned(),
            source_key: "imap:v1:u42".to_owned(),
            created_at: 1,
        });
    }

    /// Seeds the fake's snippet answer with one snippet, minted around
    /// `id`, so the listing and the search have rows to answer.
    pub(crate) fn holding_snippet(&self, id: &str, title: &str, body: &str) {
        self.assist.lock().unwrap().snippets.push(Snippet {
            id: astra_ai::SnippetId::new(id.to_owned()),
            title: title.to_owned(),
            body: body.to_owned(),
            created_at: 1,
            updated_at: 1,
        });
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
        if let Intent::Compose(compose) = &intent {
            self.play_compose(compose);
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

    async fn attachment_content(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        id: u32,
    ) -> Result<Option<AttachmentContent>, AttachmentReadError> {
        self.attachment_into.lock().unwrap().push((
            account.as_str().to_owned(),
            key.as_str().to_owned(),
            id,
        ));
        self.attachment.lock().unwrap().clone()
    }

    fn draft_status(&self) -> Option<DraftStatusSnapshot> {
        self.draft_status.lock().unwrap().clone()
    }

    fn compose_request(&self) -> Option<ComposeRequestSnapshot> {
        self.compose_request.lock().unwrap().clone()
    }

    fn take_compose_request(&self) -> Option<ComposeRequestSnapshot> {
        self.compose_request.lock().unwrap().take()
    }

    async fn open_mailto(&self, uri: &str) -> Option<MailtoOpen> {
        self.mailto_into.lock().unwrap().push(uri.to_owned());
        self.mailto_open.lock().unwrap().clone()
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

    fn drain_new_mail(&self) -> NewMailBatch {
        let batch = self.batches.lock().unwrap().pop_front().unwrap_or_default();
        self.drained
            .lock()
            .unwrap()
            .push(batch.messages.len() + batch.overflow);
        batch
    }

    fn record_pass_outcome(&self, account: &AccountId, outcome: PassOutcome) {
        self.outcomes
            .lock()
            .unwrap()
            .push((account.as_str().to_owned(), outcome));
    }

    async fn wait_first_pass(&self, account: &AccountId) {
        self.first_waits
            .lock()
            .unwrap()
            .push(account.as_str().to_owned());
    }

    async fn draft_chips(&self, account: &AccountId, key: &ProviderKey) -> Vec<Chip> {
        self.chips_into
            .lock()
            .unwrap()
            .push((account.as_str().to_owned(), key.as_str().to_owned()));
        self.chips
            .lock()
            .unwrap()
            .iter()
            .find(|((held, held_key), _)| {
                held.as_str() == account.as_str() && held_key.as_str() == key.as_str()
            })
            .map_or_else(Vec::new, |(_, chips)| chips.clone())
    }

    async fn summarise_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<String, SummariseMessageError> {
        self.summarise_into
            .lock()
            .unwrap()
            .push((account.as_str().to_owned(), key.as_str().to_owned()));
        self.summary
            .lock()
            .unwrap()
            .clone()
            .ok_or(SummariseMessageError::Unresolved)
    }

    async fn account_style_note(&self, account: &AccountId) -> Option<String> {
        self.style_into
            .lock()
            .unwrap()
            .push(account.as_str().to_owned());
        self.style_note.lock().unwrap().clone()
    }

    fn dismiss_chips(&self, account: &AccountId, key: &ProviderKey) {
        let asked = (account.as_str().to_owned(), key.as_str().to_owned());
        self.dismissed.lock().unwrap().push(asked.clone());
        self.chips
            .lock()
            .unwrap()
            .retain(|(held, _)| *held != asked);
    }

    async fn compose_from_draft(&self, account: &AccountId, key: &ProviderKey, draft: &str) {
        self.drafted_into.lock().unwrap().push((
            account.as_str().to_owned(),
            key.as_str().to_owned(),
            draft.to_owned(),
        ));
        // The publish the core's own accept makes: the reply's prefill into
        // the single slot, the draft's paragraphs riding the real snapshot the
        // app core's own tests pin.
        *self.compose_request.lock().unwrap() =
            Some(compose_prefill(ComposeKind::Reply, account.as_str()));
    }

    fn knowledge_list(&self, account: &AccountId) -> Vec<KnowledgeView> {
        let mut assist = self.assist.lock().unwrap();
        assist.knowledge_into.push(account.as_str().to_owned());
        assist.knowledge.clone()
    }

    async fn knowledge_extract(&self, account: &AccountId, limit: usize) -> ExtractionSummary {
        let mut assist = self.assist.lock().unwrap();
        assist
            .extracted_into
            .push((account.as_str().to_owned(), limit));
        assist.extraction
    }

    fn knowledge_forget(&self, id: &KnowledgeId) -> bool {
        let mut assist = self.assist.lock().unwrap();
        assist.forgotten_into.push(id.as_str().to_owned());
        let before = assist.knowledge.len();
        assist.knowledge.retain(|entry| entry.id != *id);
        before != assist.knowledge.len()
    }

    fn snippet_list(&self) -> Vec<Snippet> {
        self.assist.lock().unwrap().snippets.clone()
    }

    async fn snippet_search(&self, query: &str) -> Vec<Snippet> {
        let mut assist = self.assist.lock().unwrap();
        assist.searched_into.push(query.to_owned());
        assist.snippets.clone()
    }

    async fn compose_autocomplete(&self, before: &str, after: &str) -> String {
        let mut assist = self.assist.lock().unwrap();
        assist
            .suggested_into
            .push((before.to_owned(), after.to_owned()));
        assist.suggestion.clone()
    }

    async fn translate_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        lang: TranslateLanguage,
    ) -> String {
        let mut assist = self.assist.lock().unwrap();
        assist.translated_into.push((
            account.as_str().to_owned(),
            key.as_str().to_owned(),
            lang.as_str().to_owned(),
        ));
        assist.translation.clone()
    }

    async fn generate_digest(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError> {
        let mut docs = self.docs.lock().unwrap();
        docs.generated_into.push((
            account.as_str().to_owned(),
            AiDocumentKind::Digest.as_str().to_owned(),
            days,
        ));
        docs.generated.clone()
    }

    async fn generate_report(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError> {
        let mut docs = self.docs.lock().unwrap();
        docs.generated_into.push((
            account.as_str().to_owned(),
            AiDocumentKind::Report.as_str().to_owned(),
            days,
        ));
        docs.generated.clone()
    }

    fn latest_document(&self, account: &AccountId, kind: AiDocumentKind) -> Option<DocumentView> {
        let mut docs = self.docs.lock().unwrap();
        docs.latest_into
            .push((account.as_str().to_owned(), kind.as_str().to_owned()));
        // The seeded row answers only its own kind, the way the store's
        // newest-row-per-kind read does.
        docs.latest.clone().filter(|view| view.kind == kind)
    }

    async fn split_map(&self, account: &AccountId) -> Vec<SplitEntry> {
        let mut docs = self.docs.lock().unwrap();
        docs.split_into.push(account.as_str().to_owned());
        docs.split.clone()
    }

    fn calendar(&self) -> CalendarSnapshot {
        self.calendar.lock().unwrap().clone()
    }

    async fn generate_meeting_brief(
        &self,
        account: &AccountId,
        day: CalendarDate,
    ) -> Result<MeetingBriefView, BriefDocumentError> {
        let mut brief = self.brief.lock().unwrap();
        brief
            .brief_into
            .push((account.as_str().to_owned(), day.to_string()));
        brief.brief.clone()
    }

    fn people(&self) -> PeopleSnapshot {
        self.people.lock().unwrap().clone()
    }

    fn settings(&self) -> SettingsSnapshot {
        self.settings.lock().unwrap().clone()
    }

    async fn card_seed(&self, card: &CardRef) -> Option<ContactDraftInput> {
        self.seeded_into.lock().unwrap().push((
            card.person.get(),
            card.account.as_str().to_owned(),
            card.card.as_str().to_owned(),
        ));
        self.card_seed.lock().unwrap().clone()
    }

    async fn recipient_suggestions(&self, token: &str, limit: usize) -> Vec<RecipientSuggestion> {
        self.suggested_into
            .lock()
            .unwrap()
            .push((token.to_owned(), limit));
        self.suggestions.lock().unwrap().clone()
    }

    async fn avatar_by_email(&self, email: &str) -> AvatarView {
        self.avatar_into.lock().unwrap().push(email.to_owned());
        self.avatar.lock().unwrap().clone()
    }
}
