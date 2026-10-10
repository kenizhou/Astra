// SPDX-License-Identifier: GPL-3.0-only
//! Knowledge extraction and prompt shaping: the batch job that reads an
//! account's recent mail and lands model-derived knowledge entries, and
//! the read that turns the account's stored entries into the labelled
//! lines the draft prompt carries. Extraction is one structured
//! completion over the batch (the inbox-zero knowledge and history
//! extraction shapes over Astra's seams): every message rides the batch
//! prompt in its own untrusted wrap, the completion names the message
//! each entry was drawn from so the landing row keeps its source, and
//! the insert loop skips a text the account already holds quietly, the
//! store's exact-text dedup guard speaking through the error's message.
//! A failed call or an unparseable completion lands nothing and logs;
//! extraction never crashes its caller.

use astra_ai::{
    repair_json, tag_untrusted, AiStore, CorrelationId, KnowledgeEntryRow, KnowledgeId, LlmUseCase,
    NewKnowledgeEntry,
};
use engine_api::{AccountId, ProviderKey};
use serde::Deserialize;
use std::fmt::Write as _;

use crate::ai_rules::{AiComplete, MailContext};

/// The version of the knowledge extraction prompt. Any change to what
/// [`extraction_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the extraction evals (the rulebook's AI rule; the
/// eval pins land with their slice). It rides beside the builder the way
/// the drafting and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const KNOWLEDGE_EXTRACT_VERSION: &str = "1";

/// How many messages one extraction batch carries: the account's most
/// recent mail, newest first from the caller, capped at the spec's
/// thirty so one batch bounds what untrusted mail can carry into the
/// prompt.
pub const KNOWLEDGE_EXTRACT_LIMIT: usize = 30;

/// How many body characters each batch message rides: the per-message
/// share of the prompt's mail budget, the drafting and style collectors'
/// precedent.
pub const KNOWLEDGE_BODY_CAP: usize = 2000;

/// How many entries the draft prompt's knowledge context carries: the
/// spec's ten, ordered by [`knowledge_context`].
pub const KNOWLEDGE_CONTEXT_LIMIT: usize = 10;

/// The marker a truncated body carries, so the model knows the message
/// continues past what it can see (the collectors' convention).
const TRUNCATION_MARKER: &str = "[truncated]";

/// One knowledge entry's kind, the five-way closed set the extraction
/// prompt teaches. Wire words are snake_case; an unknown word is
/// rejected loudly at parse, never silently dropped or defaulted: a
/// completion that names a kind this build does not know empties its
/// whole batch rather than guessing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    /// A stable detail about the user, their work, or their arrangements.
    Fact,
    /// How the user likes mail handled or replies written.
    Preference,
    /// An ongoing effort or plan the user is part of, with its current
    /// state.
    Project,
    /// A person the user corresponds with and how they relate.
    Contact,
    /// How the user writes and presents themselves.
    Persona,
}

impl KnowledgeKind {
    /// The closed set, in wire order: the one list the extraction prompt
    /// teaches and the pins cross-check.
    pub const ALL: [Self; 5] = [
        Self::Fact,
        Self::Preference,
        Self::Project,
        Self::Contact,
        Self::Persona,
    ];

    /// The snake_case wire word.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Project => "project",
            Self::Contact => "contact",
            Self::Persona => "persona",
        }
    }

    /// The kind from its wire word, when the word is of the closed set.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == word)
    }

    /// The kind's context order: persona first, preferences beside it,
    /// everything else behind them (the spec's kind-relevance order).
    #[must_use]
    pub fn context_rank(&self) -> u8 {
        match self {
            Self::Persona => 0,
            Self::Preference => 1,
            Self::Fact | Self::Project | Self::Contact => 2,
        }
    }
}

impl std::fmt::Display for KnowledgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One knowledge entry as the assist model sees it: the landed store row
/// with its identity halves in the app's newtypes and its kind parsed
/// into the closed set. Built by [`extract_knowledge`] from the row the
/// store answered.
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeEntry {
    /// The entry's identity, minted at extraction.
    pub id: KnowledgeId,
    /// The entry's kind, one of the closed set.
    pub kind: KnowledgeKind,
    /// The entry's text.
    pub text: String,
    /// The account owning the message the entry was extracted from.
    pub source_account: AccountId,
    /// The source message's provider key, unique within its account.
    pub source_key: ProviderKey,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
}

/// The completion's parsed shape: `{"entries": [...]}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively; an unknown kind is not, being the closed set, and the
/// `message` citation is the 1-based message number the prompt numbered
/// the batch by, oldest first.
#[derive(Debug, Deserialize)]
struct Extracted {
    /// The extracted entries, in the model's order.
    entries: Vec<ExtractedEntry>,
}

/// One extracted entry before its identity is minted.
#[derive(Debug, Deserialize)]
struct ExtractedEntry {
    /// The entry's kind, one of the closed set.
    kind: KnowledgeKind,
    /// The entry's text.
    text: String,
    /// The 1-based number of the message the entry was drawn from.
    message: usize,
}

/// The counted outcome of one extraction run: the entries that landed
/// beside how many the exact-text dedup guard skipped, the pair the
/// host's extraction pull summarises.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct KnowledgeLanding {
    /// The entries that landed, in the model's order.
    pub landed: Vec<KnowledgeEntry>,
    /// How many entries the dedup guard skipped: a text the account
    /// already held, a within-batch repeat included.
    pub skipped_duplicates: usize,
}

/// Extracts knowledge for `account` from `messages`: builds the
/// versioned batch prompt, sends it through the seam under the turn's
/// `correlation` (the knowledge extract use case, economy tier), parses
/// the completion strictly with one repair through the shared recovery
/// path, then lands every citable entry in the knowledge store with its
/// source tracked, skipping a text the account already holds quietly.
/// `messages` arrive newest first from the caller, exactly as the
/// engine's reads return them, so the batch cap keeps the newest
/// [`KNOWLEDGE_EXTRACT_LIMIT`] of them.
///
/// The answer is the entries that landed. An empty `messages` list never
/// calls; a failed call, an unparseable completion, or a batch that
/// names a kind outside the closed set lands nothing and logs, never
/// panicking: the extraction job survives its model. An entry whose
/// message citation falls outside the batch is dropped alone, the rest
/// of the batch still lands.
pub async fn extract_knowledge(
    messages: &[MailContext],
    account: &AccountId,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
    store: &AiStore,
) -> Vec<KnowledgeEntry> {
    extract_knowledge_counted(messages, account, correlation, complete, store)
        .await
        .landed
}

/// [`extract_knowledge`] with the dedup skips counted: the same run, the
/// same quiet skips, the landing answered beside how many entries the
/// guard refused so a caller can report the run without re-reading the
/// store.
pub(super) async fn extract_knowledge_counted(
    messages: &[MailContext],
    account: &AccountId,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
    store: &AiStore,
) -> KnowledgeLanding {
    if messages.is_empty() {
        return KnowledgeLanding {
            landed: Vec::new(),
            skipped_duplicates: 0,
        };
    }
    let prompt = extraction_prompt(messages);
    let completion = match complete
        .complete(LlmUseCase::KnowledgeExtract, &prompt, correlation)
        .await
    {
        Ok(completion) => completion,
        Err(err) => {
            log::warn!("ai assist: the knowledge extraction call failed; nothing lands: {err}");
            return KnowledgeLanding {
                landed: Vec::new(),
                skipped_duplicates: 0,
            };
        }
    };
    let Some(extracted) = parse_extracted(&completion) else {
        return KnowledgeLanding {
            landed: Vec::new(),
            skipped_duplicates: 0,
        };
    };
    let kept: Vec<&MailContext> = kept_messages(messages).collect();
    let mut landed = Vec::new();
    let mut skipped_duplicates = 0usize;
    for entry in extracted.entries {
        let Some(message) = entry.message.checked_sub(1).and_then(|at| kept.get(at)) else {
            log::warn!(
                "ai assist: the knowledge entry cited message {} outside the batch; it is dropped",
                entry.message,
            );
            continue;
        };
        let row = NewKnowledgeEntry {
            id: KnowledgeId::generate(),
            account: account.as_str().to_owned(),
            kind: entry.kind.as_str().to_owned(),
            text: entry.text,
            source_message_account: message.account.as_str().to_owned(),
            source_message_key: message.key.as_str().to_owned(),
            correlation: correlation.clone(),
        };
        match store.insert_knowledge(&row) {
            Ok(landed_row) => landed.push(entry_from_row(landed_row, entry.kind, message)),
            Err(err) if is_duplicate(&err) => {
                skipped_duplicates += 1;
                log::debug!(
                    "ai assist: the knowledge entry is already stored; the dedup guard skips it"
                );
            }
            Err(err) => {
                log::warn!("ai assist: the knowledge entry could not be stored: {err}");
            }
        }
    }
    KnowledgeLanding {
        landed,
        skipped_duplicates,
    }
}

/// Reads the account's knowledge context for the draft prompt: the
/// newest [`KNOWLEDGE_CONTEXT_LIMIT`] entries in the kind order the
/// context ranking pins, each as one labelled line. A read that fails
/// answers nothing with a warning, so the draft ships without knowledge
/// rather than failing on its shaping.
pub fn knowledge_context(account: &AccountId, store: &AiStore) -> Vec<String> {
    match store.knowledge_for_account(account.as_str()) {
        Ok(rows) => context_lines(&rows),
        Err(err) => {
            log::warn!(
                "ai assist: the knowledge read failed; the prompt ships without knowledge: {err}"
            );
            Vec::new()
        }
    }
}

/// The context lines for `rows`, the pure ranking the store read feeds:
/// persona first, preferences beside it, then everything else (facts,
/// projects, contacts alike, a foreign kind word included) by recency,
/// the store's newest-first order preserved inside each group; the first
/// [`KNOWLEDGE_CONTEXT_LIMIT`] entries render, one `kind: text` line
/// each, the label the draft prompt's knowledge slot rides.
pub(super) fn context_lines(rows: &[KnowledgeEntryRow]) -> Vec<String> {
    let mut ranked: Vec<&KnowledgeEntryRow> = rows.iter().collect();
    ranked.sort_by_key(|row| {
        KnowledgeKind::from_word(&row.kind).map_or(2, |kind| kind.context_rank())
    });
    ranked
        .into_iter()
        .take(KNOWLEDGE_CONTEXT_LIMIT)
        .map(|row| format!("{}: {}", row.kind, row.text))
        .collect()
}

/// Builds the versioned extraction prompt: the instruction (the closed
/// set of kinds with one definition line each, the grounding and secrecy
/// rules) and the JSON output policy ride the trusted half, and every
/// batch message rides its own untrusted wrap under its trusted message
/// number, oldest first, so the completion can cite the message each
/// entry came from and a mail body can never speak as an instruction.
#[must_use]
pub fn extraction_prompt(messages: &[MailContext]) -> String {
    let mut prompt = String::from(
        "You build a knowledge base for one email user from the messages below.\n\
         \n\
         Extract durable knowledge about the user that would help draft their replies later. \
         The closed set of kinds is fact, preference, project, contact, persona, one \
         definition each:\n\
         fact: a stable detail about the user, their work, or their arrangements.\n\
         preference: how the user likes mail handled or replies written.\n\
         project: an ongoing effort or plan the user is part of, with its current state.\n\
         contact: a person the user corresponds with and how they relate.\n\
         persona: how the user writes and presents themselves.\n\
         \n\
         Ground every entry in the messages; invent nothing.\n\
         Write each entry as one short third-person sentence about the user.\n\
         Record the message number the entry was drawn from.\n\
         Do not record passwords, tokens, or account numbers.\n\
         When nothing durable is there, return an empty list.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: \
         {\"entries\":[{\"kind\":\"fact\",\"text\":\"...\",\"message\":1}]}.\n\
         \n\
         Here are the messages, oldest first:\n",
    );
    for (at, message) in kept_messages(messages).enumerate() {
        let _ = write!(
            prompt,
            "Message {}:\n{}\n",
            at + 1,
            tag_untrusted("email", &rendered(message)),
        );
    }
    prompt.push_str("\nExtract the knowledge entries from the messages above.\n");
    prompt
}

/// The messages the batch keeps: the newest [`KNOWLEDGE_EXTRACT_LIMIT`]
/// entries of the newest-first list the caller collected, flipped to
/// oldest-first for rendering.
fn kept_messages(messages: &[MailContext]) -> impl Iterator<Item = &MailContext> {
    messages.iter().take(KNOWLEDGE_EXTRACT_LIMIT).rev()
}

/// One batch message as the prompt renders it: the sender, the subject,
/// and the body capped at [`KNOWLEDGE_BODY_CAP`] characters with the
/// truncation marker when the message continues past the cap.
fn rendered(mail: &MailContext) -> String {
    let from = if mail.from_name.trim().is_empty() {
        mail.from_address.clone()
    } else {
        format!("{} <{}>", mail.from_name, mail.from_address)
    };
    format!(
        "From: {from}\nSubject: {}\n\n{}",
        mail.subject,
        capped(&mail.body),
    )
}

/// `body`'s head to the cap with the truncation marker when it ran over,
/// char boundaries respected; an under-cap body rides unchanged.
fn capped(body: &str) -> String {
    if body.chars().count() <= KNOWLEDGE_BODY_CAP {
        return body.to_owned();
    }
    let head: String = body.chars().take(KNOWLEDGE_BODY_CAP).collect();
    format!("{head}\n{TRUNCATION_MARKER}")
}

/// The landed entry from its row, the kind the parse already decided and
/// the source message the citation resolved to.
fn entry_from_row(
    row: KnowledgeEntryRow,
    kind: KnowledgeKind,
    message: &MailContext,
) -> KnowledgeEntry {
    KnowledgeEntry {
        id: row.id,
        kind,
        text: row.text,
        source_account: message.account.clone(),
        source_key: message.key.clone(),
        created_at: row.created_at,
    }
}

/// Whether `err` is the store's exact-text dedup guard refusing a
/// repeated (account, text): the unique index's message, read from the
/// error's text the store leaves it in.
fn is_duplicate(err: &astra_ai::AiStoreError) -> bool {
    err.to_string().contains("UNIQUE constraint failed")
}

/// Parses one completion as the extracted entries: a strict parse first,
/// and on failure exactly one repair through the first balanced JSON
/// object, the shared recovery path; `None` when neither parse lands,
/// the whole batch emptying on an unknown kind word among them.
fn parse_extracted(completion: &str) -> Option<Extracted> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map_err(|err| {
            log::warn!(
                "ai assist: the knowledge extraction output was unparseable; nothing lands: {err}"
            );
            err
        })
        .ok()
}
