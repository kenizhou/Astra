// SPDX-License-Identifier: GPL-3.0-only
//! Snippet search tests, over the fixture completion seam: the ranked
//! path (the SnippetSearch use case under the turn's correlation, the
//! position-ordered matches, the repair path), the fallbacks (the
//! substring search over title and body on a failed or unparseable
//! call), the quiet skips (an out-of-range or repeated position), the
//! empty library and the honest empty match list, and the prompt pins
//! (the numbered library, the search text, the schema, the version).

use astra_ai::{CorrelationId, LlmUseCase, SnippetId, SnippetRow};

use super::snippets::{ranking_prompt, search_snippets, Snippet, SNIPPET_SEARCH_VERSION};
use crate::dispatch_fakes::ai::FixtureComplete;

/// One store row with every field set, `at` naming its position in the
/// library.
fn row(at: i64, title: &str, body: &str) -> SnippetRow {
    SnippetRow {
        id: SnippetId::new(format!("s{at}")),
        title: title.to_owned(),
        body: body.to_owned(),
        created_at: at,
        updated_at: at,
    }
}

/// The three-snippet library: an address block, an invoice note, and a
/// thanks line.
fn library() -> Vec<Snippet> {
    vec![
        row(0, "Address", "Here is my address: 1 Main Street.").into(),
        row(1, "Invoice details", "Please pay within fourteen days.").into(),
        row(2, "Thanks", "Thanks so much for your help!").into(),
    ]
}

/// The ids of `snippets`, in order, for the shape assertions.
fn ids(snippets: &[Snippet]) -> Vec<&str> {
    snippets.iter().map(|one| one.id.as_str()).collect()
}

#[tokio::test]
async fn a_ranked_search_answers_the_matched_snippets_in_order() {
    let complete = FixtureComplete::scripted(&[r#"{"matches":[3,1]}"#]);
    let snippets = library();
    let correlation = CorrelationId::generate();
    let matched = search_snippets("payment", &snippets, &correlation, &complete).await;
    assert_eq!(ids(&matched), vec!["s2", "s0"], "the ranking orders them");
    assert_eq!(complete.call_count(), 1, "the search is one call");
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::SnippetSearch);
    assert_eq!(call.correlation, correlation);
}

#[tokio::test]
async fn an_empty_library_answers_without_a_call() {
    let complete = FixtureComplete::scripted(&[]);
    let matched = search_snippets("anything", &[], &CorrelationId::generate(), &complete).await;
    assert!(matched.is_empty());
    assert_eq!(complete.call_count(), 0, "nothing to rank, no call");
}

#[tokio::test]
async fn a_failed_call_falls_back_to_the_substring_search() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let snippets = library();
    let matched =
        search_snippets("INVOICE", &snippets, &CorrelationId::generate(), &complete).await;
    assert_eq!(
        ids(&matched),
        vec!["s1"],
        "the fallback matches the title without case",
    );
    assert_eq!(complete.call_count(), 1, "the failed attempt still spent");
    let body = search_snippets("fourteen", &snippets, &CorrelationId::generate(), &complete).await;
    assert_eq!(ids(&body), vec!["s1"], "the fallback matches the body too");
}

#[tokio::test]
async fn an_unparseable_ranking_falls_back_to_the_substring_search() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let snippets = library();
    let matched = search_snippets("thanks", &snippets, &CorrelationId::generate(), &complete).await;
    assert_eq!(ids(&matched), vec!["s2"]);
}

#[tokio::test]
async fn an_empty_query_fallback_matches_every_snippet_in_order() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let snippets = library();
    let matched = search_snippets("", &snippets, &CorrelationId::generate(), &complete).await;
    assert_eq!(ids(&matched), vec!["s0", "s1", "s2"]);
}

#[tokio::test]
async fn an_out_of_range_or_repeated_position_is_skipped() {
    let complete = FixtureComplete::scripted(&[r#"{"matches":[9,2,2,1]}"#]);
    let snippets = library();
    let matched = search_snippets("thanks", &snippets, &CorrelationId::generate(), &complete).await;
    assert_eq!(
        ids(&matched),
        vec!["s1", "s0"],
        "the out-of-range and the repeat skip, the rest keep their order",
    );
}

#[tokio::test]
async fn an_empty_match_list_answers_no_snippets() {
    let complete = FixtureComplete::scripted(&[r#"{"matches":[]}"#]);
    let snippets = library();
    let matched =
        search_snippets("address", &snippets, &CorrelationId::generate(), &complete).await;
    assert!(
        matched.is_empty(),
        "a parseable empty ranking is an honest no-match, not a fallback",
    );
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Ranked:\n{}\nDone", r#"{"matches":[2]}"#);
    let complete = FixtureComplete::scripted(&[&answer]);
    let snippets = library();
    let matched = search_snippets("pay", &snippets, &CorrelationId::generate(), &complete).await;
    assert_eq!(ids(&matched), vec!["s1"]);
}

#[test]
fn a_row_converts_into_the_app_snippet_shape() {
    let converted: Snippet = row(7, "Hours", "We are open from nine.").into();
    assert_eq!(converted.id.as_str(), "s7");
    assert_eq!(converted.title, "Hours");
    assert_eq!(converted.body, "We are open from nine.");
    assert_eq!(converted.created_at, 7);
    assert_eq!(converted.updated_at, 7);
}

#[test]
fn the_prompt_pins_the_numbered_library_and_the_schema() {
    let prompt = ranking_prompt("pay", &library());
    assert!(
        prompt.contains("The search text:\npay"),
        "the query rides as data:\n{prompt}",
    );
    assert!(
        prompt.contains("1. Title: Address")
            && prompt.contains("2. Title: Invoice details")
            && prompt.contains("3. Title: Thanks"),
        "the library rides numbered:\n{prompt}",
    );
    assert!(
        prompt.contains("Body: Please pay within fourteen days."),
        "each body rides beside its title:\n{prompt}",
    );
    assert!(
        prompt.contains(r#"{"matches":[2,5]}"#),
        "the schema example rides:\n{prompt}",
    );
    assert_eq!(SNIPPET_SEARCH_VERSION, "1");
}
