// SPDX-License-Identifier: GPL-3.0-only
//! The snippet library's ranked search: one structured completion that
//! ranks the library for the composer's search box, with the substring
//! fallback that keeps the picker working when the call cannot run. The
//! ranking prompt numbers the library and asks for the matched positions
//! in order (the ids the spec names surface as the position each stored
//! snippet answers by, so a typo in a long id can never cost a match);
//! titles, bodies, and the search text are the writer's own words, never
//! mail content, and ride the prompt as data the way the style note and
//! rule conditions do. An empty library answers without a call; a
//! failed call or an output no parse or repair saves falls back to the
//! case-insensitive substring search over titles and bodies; a parseable
//! ranking, an empty match list included, is honoured as answered.

use astra_ai::{repair_json, CorrelationId, LlmUseCase, SnippetId, SnippetRow};
use serde::Deserialize;
use std::fmt::Write as _;

use crate::ai_rules::AiComplete;

/// The version of the snippet ranking prompt. Any change to what
/// [`ranking_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the search evals (the rulebook's AI rule; the
/// eval pins land with their slice). It rides beside the builder the way
/// the drafting and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const SNIPPET_SEARCH_VERSION: &str = "1";

/// One snippet as the assist model sees it: the store row's mirror in
/// the app's vocabulary, the shape the search ranks and the picker
/// surfaces consume. Built from [`SnippetRow`] so the host lists the
/// store once and hands the search its library.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snippet {
    /// The snippet's identity.
    pub id: SnippetId,
    /// The snippet's short title, the picker's label.
    pub title: String,
    /// The snippet's body, the text an insert places at the cursor.
    pub body: String,
    /// The store's creation stamp (unix seconds).
    pub created_at: i64,
    /// The store's touch stamp (unix seconds), fresh on every update;
    /// the library listing's recency ordering rides it.
    pub updated_at: i64,
}

impl From<SnippetRow> for Snippet {
    fn from(row: SnippetRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            body: row.body,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// The completion's parsed shape: `{"matches": [...]}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively; the positions are the 1-based numbers the prompt numbered
/// the library by.
#[derive(Debug, Deserialize)]
struct Ranked {
    /// The matched positions, best first.
    matches: Vec<usize>,
}

/// Searches `snippets` for `query`: one structured ranking call through
/// the seam under the turn's `correlation` (the snippet search use
/// case, economy tier), answered by the matched snippets in the model's
/// order. An empty library answers empty without a call; a failed call
/// or an unparseable completion falls back to the substring search; a
/// position outside the library or repeated in the ranking skips alone,
/// the rest of the answer keeping its order.
pub async fn search_snippets(
    query: &str,
    snippets: &[Snippet],
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
) -> Vec<Snippet> {
    if snippets.is_empty() {
        return Vec::new();
    }
    let prompt = ranking_prompt(query, snippets);
    match complete
        .complete(LlmUseCase::SnippetSearch, &prompt, correlation)
        .await
    {
        Ok(completion) => match parse_ranked(&completion) {
            Some(ranked) => ranked_matches(&ranked.matches, snippets),
            None => substring_matches(query, snippets),
        },
        Err(err) => {
            log::warn!(
                "ai assist: the snippet ranking call failed; the substring search answers: {err}"
            );
            substring_matches(query, snippets)
        }
    }
}

/// Builds the versioned ranking prompt: the instruction and the JSON
/// output policy ride the trusted half, the search text rides as data,
/// and the library rides numbered, title and body each, so the
/// completion answers with the positions it ranks best first.
#[must_use]
pub fn ranking_prompt(query: &str, snippets: &[Snippet]) -> String {
    let mut prompt = format!(
        "You rank a snippet library for one search.\n\
         \n\
         Each snippet below has a position number, a title, and a body.\n\
         Rank the snippets the search text most plausibly wants, best first.\n\
         Return only the positions that plausibly match; an empty list when none do.\n\
         Never invent a position outside the list.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: {{\"matches\":[2,5]}}.\n\
         \n\
         The search text:\n\
         {query}\n\
         \n\
         The snippets:\n",
    );
    for (at, snippet) in snippets.iter().enumerate() {
        let _ = write!(
            prompt,
            "{}. Title: {}\n   Body: {}\n",
            at + 1,
            snippet.title,
            snippet.body
        );
    }
    prompt.push_str("\nReturn the ranked positions for the search text above.\n");
    prompt
}

/// The snippets the ranking answered, in its order: each position
/// resolves to its snippet, a repeated or out-of-range position skips
/// alone with a log, and a snippet the answer already named never lands
/// twice.
fn ranked_matches(positions: &[usize], snippets: &[Snippet]) -> Vec<Snippet> {
    let mut matched = Vec::new();
    for position in positions {
        match position.checked_sub(1).and_then(|at| snippets.get(at)) {
            Some(snippet) => {
                if matched.iter().any(|kept: &Snippet| kept.id == snippet.id) {
                    log::debug!(
                        "ai assist: the ranking repeated position {position}; the repeat skips"
                    );
                } else {
                    matched.push(snippet.clone());
                }
            }
            None => {
                log::warn!(
                    "ai assist: the ranking cited position {position} outside the library; it skips"
                );
            }
        }
    }
    matched
}

/// The substring fallback: every snippet whose title or body carries the
/// query, case aside, in the library's own order; an empty query matches
/// everything, the picker's plain listing.
fn substring_matches(query: &str, snippets: &[Snippet]) -> Vec<Snippet> {
    let needle = query.trim().to_lowercase();
    snippets
        .iter()
        .filter(|snippet| {
            needle.is_empty()
                || snippet.title.to_lowercase().contains(&needle)
                || snippet.body.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
}

/// Parses one completion as the ranking: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object,
/// the shared recovery path; `None` when neither parse lands.
fn parse_ranked(completion: &str) -> Option<Ranked> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map_err(|err| {
            log::warn!("ai assist: the snippet ranking output was unparseable; the substring search answers: {err}");
            err
        })
        .ok()
}
