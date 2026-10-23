// SPDX-License-Identifier: GPL-3.0-only
//! The gated live AI assist suite: the A3 services proven against the
//! real model endpoint, over the host's own seam implementations. A real
//! knowledge extraction over the harness account's recent mail lands
//! entries with their sources tracked and its summary counts
//! cross-checked against the store; a real snippet ranking orders a
//! seeded library (a query with no substring overlap, so only the ranked
//! answer can be non-empty); a real autocomplete suggestion answers
//! under the chat tier; and a real translate answers twice on one
//! network call, the ledger row count the no-request proof rides.
//!
//! The gates are the earlier suites': `ASTRA_LIVE_LLM=1` for the model
//! connection, plus `ASTRA_LIVE_HARNESS=1` for the test that needs the
//! local Stalwart harness; an open gate with an unusable connection fails
//! loudly rather than skipping. The recorded invocation runs this suite
//! one test at a time (`-- --nocapture --test-threads=1`): the tests
//! share one model endpoint and one live server, and serial calls keep a
//! rate limit from failing a neighbour.
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! export ASTRA_LIVE_LLM=1
//! export ASTRA_LLM_DEFAULT=anthropic:deepseek-flash
//! export ASTRA_LLM_API_KEY_ANTHROPIC=<the key>
//! export ASTRA_LLM_BASE_URL_ANTHROPIC=https://api.deepseek.com/anthropic
//! cargo test -p astra-desktop ai::live_assist -- --nocapture --test-threads=1
//! ```

// The harness-backed support: its own file so this one stays under the
// 500-line cap the gate enforces. The explicit path because this file is
// loaded through `#[path]` and so owns no directory a bare `mod` would
// resolve into.
#[path = "live_assist/harness.rs"]
mod harness;

use std::sync::Arc;

use astra_ai::{CorrelationId, DlpPolicy, NewSnippet, SnippetId};
use astra_app::{
    autocomplete, search_snippets, translate, KnowledgeKind, Snippet, TranslateLanguage,
    KNOWLEDGE_EXTRACT_LIMIT,
};
use engine_api::{AccountId, Engine, ProviderKey};

use super::live_tests::live;
use super::seams::LlmAiComplete;
use crate::ai::fixture;

use harness::{
    assist_app, delete_subject_mail, dial, document, harness, refresh, KNOWLEDGE_SUBJECT,
    KNOWLEDGE_WORDS,
};

/// A real extraction over the harness account's recent mail: the proof
/// sends its own durable note through the product's submit path (the
/// style proof's precedent, so the batch always holds knowledge worth
/// landing beside the seed's own generic notes), refreshes, and runs the
/// host's extraction pull. The landed entries carry their sources (every
/// source key a message the engine really holds), the kinds stay inside
/// the closed set, the summary's counts cross-check against the store and
/// the account's message count, and the run spent its economy-tier ledger
/// row. The sweep before and after withdraws the proof's own copies, so
/// the harness keeps its seeded shape.
#[test]
fn extraction_lands_sourced_entries_over_the_live_account() {
    let Some((client, ledger)) = live("live-a3-extract") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();
    let ai = super::incoming_pass(
        Some(Arc::clone(&client)),
        Arc::clone(&store),
        Ok(DlpPolicy::Allow),
        None,
    )
    .expect("the pass arms over the live client");

    fixture::runtime().block_on(|| async {
        let (account, providers) = dial(&host, &port, &user, &password).await;
        let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
        let app = assist_app(Arc::clone(&engine), account.clone(), providers, Some(ai));
        // A second dial's first provider is the sweep's own handle: the
        // app keeps its providers private, and the cleanup needs one the
        // test holds.
        let (_scratch_account, cleanup_providers) = dial(&host, &port, &user, &password).await;
        let cleanup = cleanup_providers.into_iter().next().expect("a provider");

        // Back to a known state: a run interrupted before its cleanup may
        // have left the proof's copies behind, delivered or filed.
        refresh(&app).await;
        delete_subject_mail(&engine, &cleanup, &account, &[KNOWLEDGE_SUBJECT]).await;
        refresh(&app).await;

        app.dispatch(astra_app::Intent::SubmitRichMail {
            from: account.clone(),
            to: "astra1@example.test".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: KNOWLEDGE_SUBJECT.to_owned(),
            document: document(KNOWLEDGE_WORDS),
            blobs: Vec::new(),
            crypto: astra_app::CryptoFlags::default(),
        })
        .await
        .expect("the proof send");
        refresh(&app).await;

        let total = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .len();
        assert!(
            total > 4,
            "the seeded notes beside the proof's own copies rode the sync: {total} messages"
        );

        // The textless-2xx retry: a failed or unparseable run lands
        // nothing (the summary's own zeros name it), so a bounded retry
        // is honest and the store stays the truth.
        let mut summary = None;
        for _ in 0..3 {
            let run = app
                .knowledge_extract(&account, KNOWLEDGE_EXTRACT_LIMIT)
                .await;
            if run.extracted > 0 {
                summary = Some(run);
                break;
            }
        }
        let summary = summary.expect("the extraction landed entries within three tries");
        assert!(
            summary.messages_considered > 0
                && summary.messages_considered <= u32::try_from(KNOWLEDGE_EXTRACT_LIMIT).unwrap(),
            "the batch carried a sane share of the account's mail: {summary:?}"
        );
        assert_eq!(
            summary.messages_considered,
            u32::try_from(total.min(KNOWLEDGE_EXTRACT_LIMIT)).unwrap(),
            "the window read the account's whole synced set inside the cap"
        );

        let rows = store
            .knowledge_for_account(account.as_str())
            .expect("the knowledge read");
        assert_eq!(
            rows.len(),
            summary.extracted as usize,
            "the store holds exactly what the run said it landed"
        );
        let keys: Vec<String> = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .iter()
            .map(|message| message.id.key().as_str().to_owned())
            .collect();
        for row in &rows {
            assert!(
                KnowledgeKind::from_word(&row.kind).is_some(),
                "the landed kind is inside the closed set: {}",
                row.kind
            );
            assert!(
                !row.text.trim().is_empty(),
                "a landed entry carries text: {row:?}"
            );
            assert_eq!(
                row.source_message_account,
                account.as_str(),
                "the source names the account the mail belongs to"
            );
            assert!(
                keys.contains(&row.source_message_key),
                "the source key is a message the engine really holds: {row:?}"
            );
            assert!(
                !row.correlation.as_str().is_empty(),
                "the landing kept the extraction turn's correlation"
            );
        }
        assert!(
            ledger
                .rows()
                .iter()
                .any(|row| row.use_case == "knowledge_extract"
                    && row.tier == "economy"
                    && row.outcome == "ok"
                    && row.prompt_tokens > 0
                    && row.prompt_tokens < 100_000
                    && row.completion_tokens > 0),
            "the extraction spent its economy-tier ledger row with sane tokens"
        );

        // The cleanup: the proof's copies withdrawn, the harness seeded
        // again, the scratch ledger taking its knowledge rows with it.
        delete_subject_mail(&engine, &cleanup, &account, &[KNOWLEDGE_SUBJECT]).await;
        refresh(&app).await;
        refresh(&app).await;
    });
}

/// A real snippet ranking over a seeded library: the search text shares
/// no substring with any title or body, so a non-empty answer can only
/// come from the ranked call, and the invoice chase the text asks about
/// ranks first. The ranking landed its economy-tier ledger row.
#[test]
fn snippet_ranking_orders_a_seeded_library_live() {
    let Some((client, ledger)) = live("live-a3-snippets") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let store = ledger.store();
    let seeded = [
        (
            "Invoice chase",
            "Attached is the invoice again, with the new due date marked.",
        ),
        (
            "Address change",
            "We have moved: our new address is 42 Harbour Road.",
        ),
        ("Thanks", "Thanks for the quick turnaround on this."),
        (
            "Meeting reset",
            "Could we move our call to Thursday at three instead?",
        ),
    ];
    let library: Vec<Snippet> = seeded
        .iter()
        .map(|(title, body)| {
            let row = store
                .insert_snippet(&NewSnippet {
                    id: SnippetId::generate(),
                    title: (*title).to_owned(),
                    body: (*body).to_owned(),
                })
                .expect("the seeded snippet lands");
            Snippet::from(row)
        })
        .collect();
    let seam = LlmAiComplete::new(client);
    let query = "the one chasing a late payment";

    let ranked = fixture::runtime().block_on(|| async {
        // The retry: a failed or unparseable call falls back to the
        // substring search, which shares no text with the query and so
        // answers empty; a bounded retry spends another ranked call
        // instead.
        for attempt in 0..3 {
            let correlation = CorrelationId::new(format!("corr-live-a3-snippets-{attempt}"));
            let answer = search_snippets(query, &library, &correlation, &seam).await;
            let answered = ledger.rows().iter().any(|row| {
                row.correlation == correlation.as_str()
                    && row.use_case == "snippet_search"
                    && row.outcome == "ok"
            });
            if answered && !answer.is_empty() {
                return answer;
            }
        }
        panic!(
            "the ranked call answered within three tries; an empty answer could only be \
                the substring fallback, which the query cannot match"
        )
    });
    assert!(
        !ranked.is_empty(),
        "the ranked call answered a non-empty match list"
    );
    assert_eq!(
        ranked[0].title, "Invoice chase",
        "the snippet the search text asks about ranks first"
    );
    let seeded_ids: Vec<&str> = library.iter().map(|snippet| snippet.id.as_str()).collect();
    for snippet in &ranked {
        assert!(
            seeded_ids.contains(&snippet.id.as_str()),
            "every answer is a snippet the library holds: {snippet:?}"
        );
    }
    assert!(
        ledger
            .rows()
            .iter()
            .any(|row| row.use_case == "snippet_search"
                && row.tier == "economy"
                && row.outcome == "ok"
                && row.prompt_tokens > 0),
        "the ranking spent its economy-tier ledger row"
    );
}

/// A real autocomplete suggestion: the compose continuation answers
/// non-empty under the chat tier, the latency tier the gesture rides.
#[test]
fn autocomplete_answers_a_real_continuation_live() {
    let Some((client, ledger)) = live("live-a3-autocomplete") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let seam = LlmAiComplete::new(client);

    let suggestion = fixture::runtime().block_on(|| async {
        // The retry: a failed call answers the empty string, and an empty
        // answer never shows a chip, so the proof retries for a real one.
        for attempt in 0..3 {
            let correlation = CorrelationId::new(format!("corr-live-a3-autocomplete-{attempt}"));
            let answer = autocomplete(
                "Thanks for the quick turnaround on the ",
                "",
                &correlation,
                &seam,
            )
            .await;
            if !answer.trim().is_empty() {
                return answer;
            }
        }
        panic!("the autocomplete answered real words within three tries")
    });
    assert!(
        !suggestion.trim().is_empty(),
        "the suggestion is real text, not a blank"
    );
    assert!(
        ledger
            .rows()
            .iter()
            .any(|row| row.use_case == "autocomplete"
                && row.tier == "chat"
                && row.outcome == "ok"
                && row.prompt_tokens > 0),
        "the ask spent its chat-tier ledger row"
    );
}

/// A real translate, cached: the first call answers from the model and
/// the second answers identical text with no new ledger row, the count
/// the no-network-request proof rides. The first answer is real text and
/// not the original, and the run spent its default-tier row.
#[test]
fn translate_answers_twice_on_one_live_call() {
    let Some((client, ledger)) = live("live-a3-translate") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let store = ledger.store();
    let seam = LlmAiComplete::new(client);
    let account = AccountId::try_from("work").expect("the account id");
    let key = ProviderKey::new("m-live-translate").expect("the key");
    let body = "Hallo Grace, der Prototyp ist fertig und wartet auf deine \
                Rückmeldung. Viele Grüße, Ada.";
    let translate_rows = || {
        ledger
            .rows()
            .into_iter()
            .filter(|row| row.use_case == "translate")
            .count()
    };

    let first = fixture::runtime().block_on(|| async {
        // The same retry: a failed or unparseable call answers empty and
        // caches nothing, so a bounded retry is honest and the row count
        // measures real spending.
        for attempt in 0..3 {
            let correlation = CorrelationId::new(format!("corr-live-a3-translate-{attempt}"));
            let answer = translate(
                &account,
                &key,
                body,
                TranslateLanguage::English,
                &correlation,
                &seam,
                &store,
            )
            .await;
            if !answer.trim().is_empty() {
                assert_ne!(
                    answer, body,
                    "the answer is a translation, not the original"
                );
                return answer;
            }
        }
        panic!("the translate answered real text within three tries")
    });
    let spent = translate_rows();
    assert!(spent > 0, "the first translate spent its model call");
    assert!(
        ledger.rows().iter().any(|row| row.use_case == "translate"
            && row.tier == "default"
            && row.outcome == "ok"
            && row.prompt_tokens > 0),
        "the translate spent its default-tier ledger row"
    );

    let second = fixture::runtime().block_on(|| async {
        let correlation = CorrelationId::new("corr-live-a3-translate-cached".to_string());
        translate(
            &account,
            &key,
            body,
            TranslateLanguage::English,
            &correlation,
            &seam,
            &store,
        )
        .await
    });
    assert_eq!(
        translate_rows(),
        spent,
        "the cached second call made no network request"
    );
    assert_eq!(second, first, "the cached answer is the first answer");
}
