// SPDX-License-Identifier: GPL-3.0-only
//! The core's [`AppPort`] impl: the one concrete bridge between the app
//! core the commands drive and the trait the host holds, its own file so
//! the state module stays under the line cap (one trait impl for the one
//! concrete type, so the block cannot split further).

use astra_ai::{AiDocumentKind, KnowledgeId};
use astra_app::{
    App, AttachmentContent, AttachmentReadError, AvatarView, BriefDocumentError, CardRef, Chip,
    CompositionId, ContactDraftInput, DigestView, DocumentError, DocumentView, DraftResume,
    EmlContent, EmlReadError, ExtractionSummary, Intent, KnowledgeView, MeetingBriefView,
    NewMailBatch, PassOutcome, Snippet, SplitEntry, SummariseMessageError, TranslateLanguage,
};
use astra_viewmodel::{
    CalendarSnapshot, ComposeRequestSnapshot, DraftStatusSnapshot, MailboxListSnapshot, MailtoOpen,
    PeopleSnapshot, ReadingSnapshot, SendingSnapshot, SettingsSnapshot, SyncProgressSnapshot,
};
use async_trait::async_trait;
use engine_api::{AccountId, CalendarDate, ProviderKey, RecipientSuggestion};

use super::{AppPort, HostAccount, HostProvider};

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

    async fn attachment_content(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        id: u32,
    ) -> Result<Option<AttachmentContent>, AttachmentReadError> {
        App::attachment_content(self, account, key, id).await
    }

    async fn eml_content(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<EmlContent, EmlReadError> {
        App::eml_content(self, account, key).await
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

    async fn open_mailto(&self, uri: &str) -> Option<MailtoOpen> {
        App::open_mailto(self, uri).await
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

    fn drain_new_mail(&self) -> NewMailBatch {
        App::drain_new_mail(self)
    }

    fn record_pass_outcome(&self, account: &AccountId, outcome: PassOutcome) {
        App::record_pass_outcome(self, account, outcome);
    }

    async fn wait_first_pass(&self, account: &AccountId) {
        App::wait_first_pass(self, account).await;
    }
}
