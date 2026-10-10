// SPDX-License-Identifier: GPL-3.0-only
//! The assist scenario drives: the A3 features' arms of the seam-parameter
//! drive, each running its service over the same seam pair the rules-era
//! features ride, with the outcome compared in the feature's own shape.
//! The knowledge extraction lands its entries in the scenario store and
//! compares them with their sources; the snippet search resolves its
//! ranked positions to titles in order; the autocomplete compares the
//! trimmed continuation; the translate runs twice, the second read's
//! cache hit its pin (the offline fake's strict script panics when a
//! cached miss spends it, the summarise convention).

use astra_ai::{AiStore, CorrelationId, SnippetId};
use serde_json::json;

use crate::ai_rules::{AiComplete, MailContext};
use crate::{
    autocomplete, extract_knowledge, search_snippets, translate, Snippet, TranslateLanguage,
};

use super::drive::{expect, mail_context, mail_object, str_field};
use super::Scenario;

/// The knowledge extraction scenario: the fixture's batch messages
/// (newest first, the engine's read order) through the extraction seam
/// into the scenario store, the landed entries compared as their kind,
/// text, and source message key in landing order.
pub(super) async fn knowledge_extract_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    store: &AiStore,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let messages = messages_field(&scenario.inputs)?;
    let account = messages
        .first()
        .map(|message| message.account.clone())
        .ok_or_else(|| "carries no messages".to_owned())?;
    let landed = extract_knowledge(&messages, &account, correlation, complete, store).await;
    let entries: Vec<serde_json::Value> = landed
        .iter()
        .map(|entry| {
            json!({
                "kind": entry.kind.as_str(),
                "text": entry.text,
                "source": entry.source_key.as_str(),
            })
        })
        .collect();
    expect(
        &scenario.expected,
        &serde_json::json!({ "entries": entries }),
    )
}

/// The snippet search scenario: the fixture's library through the
/// ranking seam, the matched snippets compared as their titles in the
/// answer's order (the fallback's included, its order the library's
/// own).
pub(super) async fn snippet_search_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let query = str_field(&scenario.inputs, "query")?;
    let snippets = snippets_field(&scenario.inputs)?;
    let matched = search_snippets(&query, &snippets, correlation, complete).await;
    let titles: Vec<&str> = matched
        .iter()
        .map(|snippet| snippet.title.as_str())
        .collect();
    expect(
        &scenario.expected,
        &serde_json::json!({ "matches": titles }),
    )
}

/// The autocomplete scenario: the cursor's text halves through the
/// seam, the trimmed continuation compared as the suggestion.
pub(super) async fn autocomplete_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let before = str_field(&scenario.inputs, "before")?;
    let after = scenario
        .inputs
        .get("after")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let suggestion = autocomplete(&before, after, correlation, complete).await;
    expect(
        &scenario.expected,
        &serde_json::json!({ "suggestion": suggestion }),
    )
}

/// The translate scenario: the fixture's body through the translate
/// seam into the scenario store's cache, read twice so the second read
/// must answer from the cache alone, the translation compared as the
/// answer; a body that fails its call answers empty twice, which is the
/// honest shape the reading pane renders.
pub(super) async fn translate_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    store: &AiStore,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let mail = mail_context(&scenario.inputs)?;
    let text = str_field(&scenario.inputs, "text")?;
    let lang = lang_field(&scenario.inputs)?;
    let first = translate(
        &mail.account,
        &mail.key,
        &text,
        lang,
        correlation,
        complete,
        store,
    )
    .await;
    let again = translate(
        &mail.account,
        &mail.key,
        &text,
        lang,
        correlation,
        complete,
        store,
    )
    .await;
    let actual = if first == again {
        serde_json::json!({ "translation": first })
    } else {
        serde_json::json!({ "error": "cache" })
    };
    expect(&scenario.expected, &actual)
}

/// The extraction batch, newest first exactly as the engine's reads
/// return them: the mail objects of the `messages` array, in order.
fn messages_field(inputs: &serde_json::Value) -> Result<Vec<MailContext>, String> {
    let array = inputs
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "carries no messages array".to_owned())?;
    array.iter().map(mail_object).collect()
}

/// The library the search ranks: the `snippets` array's title and body
/// pairs, each minting the positional identity the drive owns so no
/// fixture spells an id.
fn snippets_field(inputs: &serde_json::Value) -> Result<Vec<Snippet>, String> {
    let array = inputs
        .get("snippets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "carries no snippets array".to_owned())?;
    array
        .iter()
        .enumerate()
        .map(|(at, one)| {
            Ok(Snippet {
                id: SnippetId::new(format!("s{}", at + 1)),
                title: str_field(one, "title")?,
                body: str_field(one, "body")?,
                created_at: 0,
                updated_at: 0,
            })
        })
        .collect()
}

/// The target language word, from the optional `lang` field; the fixed
/// set's default answers when the fixture names none, and a word
/// outside the set is a malformed fixture.
fn lang_field(inputs: &serde_json::Value) -> Result<TranslateLanguage, String> {
    let Some(word) = inputs.get("lang").and_then(serde_json::Value::as_str) else {
        return Ok(TranslateLanguage::English);
    };
    TranslateLanguage::ALL
        .iter()
        .copied()
        .find(|lang| lang.as_str() == word)
        .ok_or_else(|| format!("the lang word {word:?} is outside the fixed set"))
}
