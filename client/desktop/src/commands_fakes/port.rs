// SPDX-License-Identifier: GPL-3.0-only
//! The fake app's [`AppPort`] impl and the A3 assist pulls' seeds, split
//! out of the fixture root so that root stays under the 500-line limit:
//! one trait impl for the one fake, the inherited contract beside the
//! assist answers, everything the command tests seed or assert in one
//! place behind the fake's own locks.

use astra_ai::KnowledgeId;
use astra_app::{
    AppError, Chip, CompositionId, DraftResume, ExtractionSummary, Intent, KnowledgeView, Snippet,
    SummariseMessageError, TranslateLanguage,
};
use astra_viewmodel::{
    ComposeKind, ComposeRequestSnapshot, DraftStatusSnapshot, MailboxListSnapshot, ReadingSnapshot,
    SendingSnapshot, SyncProgressSnapshot,
};
use engine_api::{AccountId, ProviderKey};

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

    fn draft_status(&self) -> Option<DraftStatusSnapshot> {
        self.draft_status.lock().unwrap().clone()
    }

    fn compose_request(&self) -> Option<ComposeRequestSnapshot> {
        self.compose_request.lock().unwrap().clone()
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
}
