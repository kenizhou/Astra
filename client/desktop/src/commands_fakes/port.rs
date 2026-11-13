// SPDX-License-Identifier: GPL-3.0-only
//! The fake app's [`AppPort`] impl, split out of the fixture root so that
//! root stays under the 500-line limit: one trait impl for the one fake,
//! the inherited contract beside the assist answers, everything the
//! command tests seed or assert in one place behind the fake's own locks.
//! The seed records the assist answers read sit in the sibling `seeds`.

use astra_ai::{AiDocumentKind, KnowledgeId};
use astra_app::{
    AppError, AttachmentContent, AttachmentReadError, AvatarView, BriefDocumentError, CardRef,
    Chip, CompositionId, ContactDraftInput, DigestView, DocumentError, DocumentView, DraftResume,
    EmlContent, EmlReadError, ExtractionSummary, Intent, KnowledgeView, MeetingBriefView,
    NewMailBatch, PassOutcome, Snippet, SplitEntry, SummariseMessageError, TranslateLanguage,
};
use astra_viewmodel::{
    CalendarSnapshot, ComposeKind, ComposeRequestSnapshot, DraftStatusSnapshot, EventDetail,
    MailboxListSnapshot, MailtoOpen, PeopleSnapshot, ReadingSnapshot, SendingSnapshot,
    SettingsSnapshot, SyncProgressSnapshot,
};
use engine_api::{AccountId, CalendarDate, ProviderKey, RecipientSuggestion};

use crate::app_state::{AppPort, HostAccount};

use super::{compose_prefill, open_snapshot, FakeApp};

// The key import and collected-review fake's state, its own child so this
// file stays under the line cap beside the trait impl that serves them; the
// re-export keeps the `port::KeysFake` path the fixture root names unchanged.
mod keys_fake;
pub(crate) use keys_fake::KeysFake;

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

    async fn submit_reading_password(
        &self,
        message: &astra_app::MessageRef,
        _passphrase: astra_crypto_core::SecretBox<String>,
    ) -> Option<astra_viewmodel::PasswordPromptState> {
        // The passphrase is never recorded: it stays in-process. Only the
        // message identity the command handed through is asserted by tests.
        self.password_into.lock().unwrap().push((
            message.account.as_str().to_owned(),
            message.key.as_str().to_owned(),
        ));
        *self.password_prompt.lock().unwrap()
    }

    async fn import_key(
        &self,
        account: &AccountId,
        bytes: Vec<u8>,
        _passphrase: Option<astra_crypto_core::SecretBox<String>>,
    ) -> Result<astra_crypto_store::CryptoKeyRow, String> {
        // The passphrase is never recorded: it stays in-process. Only the
        // account and the byte count the command handed through are asserted.
        let mut keys = self.keys.lock().unwrap();
        keys.import_into
            .push((account.as_str().to_owned(), bytes.len()));
        keys.import
            .clone()
            .unwrap_or_else(|| Err("the fake holds no import answer".to_owned()))
    }

    async fn list_keys(
        &self,
        account: &AccountId,
    ) -> Result<Vec<astra_crypto_store::CryptoKeyRow>, String> {
        let mut keys = self.keys.lock().unwrap();
        keys.list_into.push(account.as_str().to_owned());
        keys.list.clone().unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn list_collected(
        &self,
        account: &AccountId,
    ) -> Result<Vec<astra_crypto_store::CollectedKeyRow>, String> {
        let mut keys = self.keys.lock().unwrap();
        keys.collected_into.push(account.as_str().to_owned());
        keys.collected.clone().unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn promote_collected(
        &self,
        account: &AccountId,
        standard: astra_crypto_core::Standard,
        fingerprint: &str,
        email: Option<&str>,
        verified: bool,
    ) -> Result<astra_crypto_store::CryptoKeyRow, String> {
        let mut keys = self.keys.lock().unwrap();
        keys.promote_into.push((
            account.as_str().to_owned(),
            standard.as_str().to_owned(),
            fingerprint.to_owned(),
            email.map(str::to_owned),
            verified,
        ));
        keys.promote
            .clone()
            .unwrap_or_else(|| Err("the fake holds no promote answer".to_owned()))
    }

    async fn reject_collected(
        &self,
        account: &AccountId,
        id: i64,
        standard: astra_crypto_core::Standard,
        fingerprint: &str,
        email: Option<&str>,
    ) -> Result<(), String> {
        let mut keys = self.keys.lock().unwrap();
        keys.reject_into.push((
            account.as_str().to_owned(),
            id,
            standard.as_str().to_owned(),
            fingerprint.to_owned(),
            email.map(str::to_owned),
        ));
        keys.reject.clone().unwrap_or(Ok(()))
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

    async fn eml_content(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<EmlContent, EmlReadError> {
        self.eml_into
            .lock()
            .unwrap()
            .push((account.as_str().to_owned(), key.as_str().to_owned()));
        self.eml.lock().unwrap().clone()
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

    async fn resume_draft_by_key(
        &self,
        message: &astra_app::MessageRef,
        composition: CompositionId,
    ) -> Option<DraftResume> {
        self.resumed_keys.lock().unwrap().push((
            message.account.as_str().to_owned(),
            message.key.as_str().to_owned(),
            composition.as_str().to_owned(),
        ));
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

    async fn refresh_calendars_if_changed(&self) {
        // The command surface never drives the periodic refresh; the timer
        // host owns it, so the fake answers the empty visit.
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

    async fn calendar_event_detail(&self, event: &astra_app::EventRef) -> Option<EventDetail> {
        self.detail_into.lock().unwrap().push((
            event.account.as_str().to_owned(),
            event.key.as_str().to_owned(),
        ));
        self.event_detail.lock().unwrap().clone()
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

    fn record_analytics(&self, event: &astra_app::AnalyticsEvent) {
        self.analytics.lock().unwrap().push(event.clone());
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
