// SPDX-License-Identifier: GPL-3.0-only
//! The cached translate action: one versioned completion through the
//! [`AiComplete`] seam that renders the open message's body into the
//! chosen target language, its answer served from the text cache
//! (migration 0009) whenever the translated body is still the body in
//! front of the reader. The prompt keeps its halves apart the way every
//! Astra AI prompt does: the instruction with the named target language
//! and the JSON output policy ride the trusted half, and the body rides
//! its own untrusted wrap, so a mail body can never speak as an
//! instruction. A cache hit returns without a seam call; a miss runs the
//! Translate use case once and puts the answer back; a failed or
//! unparseable call answers an empty string and caches nothing, so the
//! reader's retry spends the call the failure wasted.

use astra_ai::{repair_json, tag_untrusted, AiStore, CorrelationId, LlmUseCase};
use engine_api::{AccountId, ProviderKey};
use serde::Deserialize;

use crate::ai_rules::AiComplete;

/// The version of the translate prompt. Any change to what
/// [`translate_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the translate evals (the rulebook's AI rule; the
/// eval pins land with their slice). It rides beside the builder the way
/// the drafting and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const TRANSLATE_VERSION: &str = "1";

/// The cache kind word the translate action keys its text cache rows by,
/// the summarise convention's word grown a kind.
pub const TRANSLATE_CACHE_KIND: &str = "translate";

/// One translate action's target language, the fixed list the spec pins.
/// Wire words are snake_case (the cache's lang key among them); the
/// default target is English.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TranslateLanguage {
    /// English, the default target.
    #[default]
    English,
    /// Chinese.
    Chinese,
    /// German.
    German,
    /// French.
    French,
    /// Japanese.
    Japanese,
    /// Spanish.
    Spanish,
}

impl TranslateLanguage {
    /// The fixed set, in wire order: the one list the reading pane's
    /// picker offers and the pins cross-check.
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Chinese,
        Self::German,
        Self::French,
        Self::Japanese,
        Self::Spanish,
    ];

    /// The snake_case wire word, the cache's lang key.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::English => "english",
            Self::Chinese => "chinese",
            Self::German => "german",
            Self::French => "french",
            Self::Japanese => "japanese",
            Self::Spanish => "spanish",
        }
    }

    /// The language's own name, the word the prompt names its target by.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "Chinese",
            Self::German => "German",
            Self::French => "French",
            Self::Japanese => "Japanese",
            Self::Spanish => "Spanish",
        }
    }
}

impl std::fmt::Display for TranslateLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The completion's parsed shape: `{"translation": "..."}`, the object
/// the output policy pins. Unknown fields are tolerated so the shape
/// grows additively.
#[derive(Debug, Deserialize)]
struct Translated {
    /// The translated text.
    translation: String,
}

/// Translates `text` into `lang`: the cached answer when the store holds
/// a row for the message's (kind, lang) key whose body hash matches the
/// body in front of the reader, else one completion through the seam
/// under the turn's `correlation` (the translate use case, default
/// tier), parsed strictly with one repair through the shared recovery
/// path, then cached under the hash the body hashed to. The cache is an
/// optimisation, never a gate: a translation that cannot be cached still
/// answers.
///
/// An empty body answers an empty string without a call; a failed call
/// or an output no parse or repair could save answers an empty string
/// and caches nothing.
pub async fn translate(
    account: &AccountId,
    key: &ProviderKey,
    text: &str,
    lang: TranslateLanguage,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
    cache: &AiStore,
) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let hash = crate::body_hash(text);
    if let Some(hit) = cached(account, key, lang, &hash, cache) {
        return hit;
    }
    let prompt = translate_prompt(text, lang);
    let completion = match complete
        .complete(LlmUseCase::Translate, &prompt, correlation)
        .await
    {
        Ok(completion) => completion,
        Err(err) => {
            log::warn!("ai assist: the translate call failed; no translation rides: {err}");
            return String::new();
        }
    };
    let Some(translation) = parse_translation(&completion) else {
        return String::new();
    };
    if let Err(err) = cache.cache_text(
        account.as_str(),
        key.as_str(),
        TRANSLATE_CACHE_KIND,
        Some(lang.as_str()),
        &hash,
        &translation,
    ) {
        log::warn!("ai assist: the translation could not be cached: {err}");
    }
    translation
}

/// Builds the versioned translate prompt: the instruction with the
/// target language named and the JSON output policy ride the trusted
/// half, and the body rides its own untrusted wrap, the message content
/// whole so the translation misses nothing.
#[must_use]
pub fn translate_prompt(text: &str, lang: TranslateLanguage) -> String {
    format!(
        "You translate one email into the target language named below.\n\
         \n\
         Translate the message content faithfully and completely; keep the paragraph breaks.\n\
         Do not add commentary, notes, or the original text.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: {{\"translation\":\"...\"}}.\n\
         \n\
         Target language: {label}\n\
         \n\
         Translate the message in the context:\n\
         \n\
         {}",
        tag_untrusted("email body", text),
        label = lang.label(),
    )
}

/// The cached answer, when the store holds one for the body and language
/// in front of the reader; a failed read answers none so the action
/// translates fresh rather than failing on its optimisation.
fn cached(
    account: &AccountId,
    key: &ProviderKey,
    lang: TranslateLanguage,
    hash: &str,
    cache: &AiStore,
) -> Option<String> {
    match cache.cached_text(
        account.as_str(),
        key.as_str(),
        TRANSLATE_CACHE_KIND,
        Some(lang.as_str()),
        hash,
    ) {
        Ok(hit) => hit,
        Err(err) => {
            log::warn!("ai assist: the translate cache read failed; the action runs fresh: {err}");
            None
        }
    }
}

/// Parses one completion as the translation: a strict parse first, and
/// on failure exactly one repair through the first balanced JSON object,
/// the shared recovery path; `None` when neither parse lands.
fn parse_translation(completion: &str) -> Option<String> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map(|translated: Translated| translated.translation)
        .map_err(|err| {
            log::warn!(
                "ai assist: the translate output was unparseable; no translation rides: {err}"
            );
            err
        })
        .ok()
}
