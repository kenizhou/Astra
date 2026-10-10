// SPDX-License-Identifier: GPL-3.0-only
//! The AI assist model: the A3 services over the A1/A2 seams, the
//! store-facing half beside the replies model. This module owns the
//! batch knowledge extraction and the draft prompt's knowledge context,
//! the snippet library's ranked search with its substring fallback, the
//! compose autocomplete suggestion, and the cached translate action; the
//! host commands, the composer and reading-pane surfaces, and the CLI
//! verbs arrive in their own slices and consume these services through
//! the same [`AiComplete`](crate::ai_rules::AiComplete) seam the rules
//! and replies models ride.

mod autocomplete;
mod knowledge;
mod snippets;
mod translate;

#[cfg(test)]
#[path = "autocomplete_tests.rs"]
mod autocomplete_tests;

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod knowledge_tests;

#[cfg(test)]
#[path = "snippets_tests.rs"]
mod snippets_tests;

#[cfg(test)]
#[path = "translate_tests.rs"]
mod translate_tests;

pub use autocomplete::{autocomplete, autocomplete_prompt, AUTOCOMPLETE_VERSION};
pub use knowledge::{
    extract_knowledge, extraction_prompt, knowledge_context, KnowledgeEntry, KnowledgeKind,
    KNOWLEDGE_BODY_CAP, KNOWLEDGE_CONTEXT_LIMIT, KNOWLEDGE_EXTRACT_LIMIT,
    KNOWLEDGE_EXTRACT_VERSION,
};
pub use snippets::{ranking_prompt, search_snippets, Snippet, SNIPPET_SEARCH_VERSION};
pub use translate::{
    translate, translate_prompt, TranslateLanguage, TRANSLATE_CACHE_KIND, TRANSLATE_VERSION,
};
