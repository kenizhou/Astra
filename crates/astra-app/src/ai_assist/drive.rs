// SPDX-License-Identifier: GPL-3.0-only
//! The AI assist host pulls: the A3 services' app-facing halves, the
//! reads and actions the desktop commands ride. The knowledge listing
//! and the forget write read and delete through the bundle's one store;
//! the extraction pull collects the account's recent mail newest first
//! (the engine's window read, each message resolved with its body
//! fetched, the style collection's own pattern) and hands the batch to
//! the extraction service under a correlation minted for the pull's own
//! turn, answering the run's summary; the snippet listing reads the
//! store and the search hands the whole library to the ranked search,
//! its substring fallback answering when the call cannot run; the
//! autocomplete pull runs its service under a minted correlation; and
//! the translate action resolves the message's body through the reading
//! path and serves the cached answer through the text cache. Without
//! the AI bundle every pull answers its quiet empty: no store, nothing
//! to read, and no call to spend.

use astra_ai::{AiStore, CorrelationId, KnowledgeEntryRow, KnowledgeId};
use engine_api::{AccountId, Provider, ProviderKey};

use super::autocomplete::autocomplete;
use super::knowledge::extract_knowledge_counted;
use super::snippets::{search_snippets, Snippet};
use super::translate::{translate, TranslateLanguage};
use crate::ai_rules::MailContext;
use crate::reference::MessageRef;
use crate::App;

/// One knowledge entry as the host lists it: the store row's wire shape,
/// the kind riding as its word so a foreign word renders as it stands
/// rather than being dropped or guessed into the closed set.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeView {
    /// The entry's identity, minted at extraction.
    pub id: KnowledgeId,
    /// The entry's kind word: the extraction vocabulary's snake_case
    /// word, the closed set's own spelling.
    pub kind: String,
    /// The entry's text.
    pub text: String,
    /// The account owning the message the entry was extracted from.
    pub source_account: String,
    /// The source message's provider key, unique within its account.
    pub source_key: String,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

impl From<KnowledgeEntryRow> for KnowledgeView {
    fn from(row: KnowledgeEntryRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            text: row.text,
            source_account: row.source_message_account,
            source_key: row.source_message_key,
            created_at: row.created_at,
        }
    }
}

/// One extraction run's answer to the host pull: what landed, what the
/// dedup guard skipped, and how many messages the batch carried, the
/// three numbers a settings surface reports of a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtractionSummary {
    /// How many entries landed in the store.
    pub extracted: u32,
    /// How many entries the exact-text dedup guard skipped: texts the
    /// account already held, within-batch repeats included.
    pub skipped_duplicates: u32,
    /// How many messages the batch carried, after the caller's limit and
    /// the cap.
    pub messages_considered: u32,
}

impl<P: Provider> App<P> {
    /// Every knowledge entry the account holds, newest first, as the
    /// host lists it. A read that fails answers empty with a warning;
    /// no bundle, no store, no entries.
    pub fn knowledge_list(&self, account: &AccountId) -> Vec<KnowledgeView> {
        let Some(pass) = self.ai.as_ref() else {
            return Vec::new();
        };
        match pass.store.knowledge_for_account(account.as_str()) {
            Ok(rows) => rows.into_iter().map(KnowledgeView::from).collect(),
            Err(err) => {
                log::warn!("ai assist: the knowledge listing read failed: {err}");
                Vec::new()
            }
        }
    }

    /// Runs one knowledge extraction over the account's most recent
    /// mail: the newest `limit` messages (the caller's limit, already
    /// capped where the command reads it), each resolved with its plain
    /// body, handed to the extraction service under a correlation minted
    /// for the pull's own turn. A message whose body cannot be fetched
    /// rides nothing, the incoming pass's skip precedent. The answer is
    /// the run's summary; a failed call or an unparseable completion
    /// lands nothing, which the summary's own zeros name.
    pub async fn knowledge_extract(&self, account: &AccountId, limit: usize) -> ExtractionSummary {
        let Some(pass) = self.ai.as_ref() else {
            return ExtractionSummary::default();
        };
        let messages = self.recent_messages(account, limit).await;
        let considered = u32::try_from(messages.len()).unwrap_or(u32::MAX);
        let correlation = CorrelationId::generate();
        let landing = extract_knowledge_counted(
            &messages,
            account,
            &correlation,
            pass.complete.as_ref(),
            &pass.store,
        )
        .await;
        ExtractionSummary {
            extracted: u32::try_from(landing.landed.len()).unwrap_or(u32::MAX),
            skipped_duplicates: u32::try_from(landing.skipped_duplicates).unwrap_or(u32::MAX),
            messages_considered: considered,
        }
    }

    /// Forgets one knowledge entry by its identity, answering whether
    /// any row went. Entries never auto-delete; this is the only exit,
    /// and a failed delete answers false with a warning rather than
    /// pretending the row went.
    pub fn knowledge_forget(&self, id: &KnowledgeId) -> bool {
        let Some(pass) = self.ai.as_ref() else {
            return false;
        };
        match pass.store.forget_knowledge(id) {
            Ok(removed) => removed,
            Err(err) => {
                log::warn!("ai assist: the knowledge forget failed: {err}");
                false
            }
        }
    }

    /// Every snippet, the most recently touched first: the composer
    /// picker's plain listing read. A failed read answers empty with a
    /// warning; no bundle, an empty library.
    pub fn snippet_list(&self) -> Vec<Snippet> {
        let Some(pass) = self.ai.as_ref() else {
            return Vec::new();
        };
        Self::library(&pass.store)
    }

    /// Searches the snippet library: the whole store listing handed to
    /// the ranked search under a correlation minted for the search's own
    /// turn, the substring fallback answering when the ranking call
    /// cannot run. An empty library answers empty without a call.
    pub async fn snippet_search(&self, query: &str) -> Vec<Snippet> {
        let Some(pass) = self.ai.as_ref() else {
            return Vec::new();
        };
        let library = Self::library(&pass.store);
        let correlation = CorrelationId::generate();
        search_snippets(query, &library, &correlation, pass.complete.as_ref()).await
    }

    /// The compose autocomplete suggestion for the editor's text around
    /// the cursor: one short continuation under a correlation minted for
    /// the ask. A failed call answers an empty string, so the editor's
    /// chip never shows; the caller owns when to ask and the
    /// gesture-only insert.
    pub async fn compose_autocomplete(&self, before: &str, after: &str) -> String {
        let Some(pass) = self.ai.as_ref() else {
            return String::new();
        };
        let correlation = CorrelationId::generate();
        autocomplete(before, after, &correlation, pass.complete.as_ref()).await
    }

    /// Translates the message's body into `lang`: the body resolved
    /// through the reading path, the answer served through the text
    /// cache, a miss running the translate service once under a
    /// correlation minted for the action's own turn. An empty answer
    /// names a message that no longer resolves or a call that failed;
    /// the reader's retry spends the call the failure wasted.
    pub async fn translate_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
        lang: TranslateLanguage,
    ) -> String {
        let Some(pass) = self.ai.as_ref() else {
            return String::new();
        };
        let Some(body) = self.fetched_body(account, key).await else {
            log::warn!("ai assist: the message could not be resolved for translating");
            return String::new();
        };
        let correlation = CorrelationId::generate();
        translate(
            account,
            key,
            &body,
            lang,
            &correlation,
            pass.complete.as_ref(),
            &pass.store,
        )
        .await
    }

    /// The snippet library as the store holds it, newest touched first;
    /// a failed read answers an empty library with a warning, the search
    /// answering empty rather than failing on its own listing.
    fn library(store: &AiStore) -> Vec<Snippet> {
        match store.list_snippets() {
            Ok(rows) => rows.into_iter().map(Snippet::from).collect(),
            Err(err) => {
                log::warn!("ai assist: the snippet listing read failed: {err}");
                Vec::new()
            }
        }
    }

    /// The account's most recent mail as extraction contexts: the newest
    /// `limit` rows of the engine's window read (newest first, exactly
    /// the order the extraction service's batch cap expects), each
    /// resolved through the reading path with its plain body fetched. A
    /// row whose body cannot be fetched is skipped whole, the incoming
    /// pass's precedent; an account that is gone or holds no provider
    /// answers empty.
    async fn recent_messages(&self, account: &AccountId, limit: usize) -> Vec<MailContext> {
        let rows = match self
            .engine
            .mail_window(std::slice::from_ref(account), limit)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("ai assist: the extraction's window read failed: {err}");
                return Vec::new();
            }
        };
        let mut contexts = Vec::new();
        for row in &rows {
            let Some(body) = self.fetched_body(account, &row.mail.key).await else {
                log::warn!("ai assist: an extraction candidate's body fetch failed; it is skipped");
                continue;
            };
            contexts.push(MailContext {
                account: account.clone(),
                key: row.mail.key.clone(),
                from_name: row.mail.from_name.clone().unwrap_or_default(),
                from_address: row.mail.from_addr.clone().unwrap_or_default(),
                subject: row.mail.subject.clone().unwrap_or_default(),
                body,
            });
        }
        contexts
    }

    /// One message's plain body, resolved fresh by key through the
    /// reading path: the fetch the extraction candidates and the
    /// translate action share, the reply driver's own body read.
    /// `None` when the message no longer resolves, its account is gone,
    /// holds no provider, or the body cannot be fetched.
    async fn fetched_body(&self, account: &AccountId, key: &ProviderKey) -> Option<String> {
        let handle = self.account_handle(account).await?;
        let provider = handle.providers.first()?;
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        let original = crate::reading::resolve(&self.engine, &message).await?;
        let body = self
            .engine
            .message_body(provider, account, &original)
            .await
            .ok()?;
        Some(body.plain().map(str::to_owned).unwrap_or_default())
    }
}
