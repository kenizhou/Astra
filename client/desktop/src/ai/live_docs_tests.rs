// SPDX-License-Identifier: GPL-3.0-only
//! The gated live AI documents suite: the A4 services proven against the
//! real model endpoint, over the host's own seam implementations. A real
//! digest over the harness account's seeded notes (the incoming pass's
//! decided categorisation riding as the rollup source) lands non-empty
//! content, its stored row under the quantised window, and a sane
//! default-tier ledger cost; a real report parses its three sections and
//! stores under the same honest window; and the split map serves over
//! the pass's real results with no model call spent at all, the pure
//! derivation the design pins.
//!
//! The gates are the earlier suites': `ASTRA_LIVE_LLM=1` for the model
//! connection, plus `ASTRA_LIVE_HARNESS=1` for the local Stalwart
//! harness; an open gate with an unusable connection fails loudly rather
//! than skipping. The recorded invocation runs this suite one test at a
//! time (`-- --nocapture --test-threads=1`): the tests share one model
//! endpoint and one live server, and serial calls keep a rate limit from
//! failing a neighbour.
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! export ASTRA_LIVE_LLM=1
//! export ASTRA_LLM_DEFAULT=anthropic:deepseek-flash
//! export ASTRA_LLM_API_KEY_ANTHROPIC=<the key>
//! export ASTRA_LLM_BASE_URL_ANTHROPIC=https://api.deepseek.com/anthropic
//! cargo test -p astra-desktop ai::live_docs -- --nocapture --test-threads=1
//! ```

// The harness-backed support: its own file so this one stays under the
// 500-line cap the gate enforces. The explicit path because this file is
// loaded through `#[path]` and so owns no directory a bare `mod` would
// resolve into.
#[path = "live_docs/harness.rs"]
mod harness;

use std::sync::Arc;

use astra_ai::{AiDocumentKind, DlpPolicy};
use astra_app::{side_for, window_bounds, App, Intent, MessageRef, DOCUMENT_WINDOW_MAX_DAYS};
use engine_api::{AccountId, Engine, Provider};

use super::incoming_pass;
use super::live_tests::live;
use crate::ai::fixture;
use crate::ai::LlmClient;

use harness::{
    civil_of, civil_unix, dial, docs_app, harness, note_refs, reset_notes_unread, select_inbox,
    wait_categorised, NOTES,
};

/// The window every proof generates over: the clamp's ceiling, so a
/// harness seeded days back still falls inside the period. The window's
/// own bounds are pinned by the quantisation assertion below, not by the
/// seed's age.
const DAYS: u32 = DOCUMENT_WINDOW_MAX_DAYS;

/// The app the proofs share, typed once: the in-memory engine the test
/// holds beside the app, over the dialled harness account's provider.
type DocsApp = Arc<App<Box<dyn Provider>>>;

/// Arms the incoming pass over the live client and builds the app over
/// the dialled harness account: the engine the caller holds beside it,
/// the shape the desktop host assembles.
async fn armed_app(
    client: &Arc<LlmClient>,
    store: &Arc<astra_ai::AiStore>,
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (Arc<Engine>, AccountId, DocsApp) {
    let ai = incoming_pass(
        Some(Arc::clone(client)),
        Arc::clone(store),
        Ok(DlpPolicy::Allow),
        None,
    )
    .expect("the pass arms over the live client");
    let (account, providers) = dial(host, port, user, password).await;
    let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
    let app = docs_app(Arc::clone(&engine), account.clone(), providers, Some(ai));
    (engine, account, app)
}

/// Refreshes, resets the seeded notes to unread, refreshes again, and
/// runs one incoming pass over the reset state, waiting for its decided
/// categorisation: the rollup source and the split derivation both ride
/// those rows. The driver is aborted once the rows stand; the notes'
/// references answer for the window assertions.
async fn categorise_the_notes(
    app: &DocsApp,
    store: &Arc<astra_ai::AiStore>,
    account: &AccountId,
) -> Vec<MessageRef> {
    app.dispatch(Intent::RefreshMail)
        .await
        .expect("the refresh");
    select_inbox(app, account).await;
    reset_notes_unread(app).await;
    app.dispatch(Intent::RefreshMail)
        .await
        .expect("the reset refresh");
    let driver = tokio::spawn({
        let app = Arc::clone(app);
        async move { app.drive_incoming_pass().await }
    });
    let notes = note_refs(app);
    assert_eq!(
        notes.len(),
        NOTES.len(),
        "the three seeded notes are listed"
    );
    wait_categorised(store, account, &notes).await;
    driver.abort();
    notes
}

/// A real digest over the seeded notes: the pass's decided categorisation
/// rides as the rollup source, the one generation answers real text, the
/// stored row carries it under the quantised window that really covers
/// the notes, and the ledger names the call with a sane default-tier
/// cost. The account's own Sent mail rides nothing of the set (the
/// recorded filter), so the digest summarises what arrived.
#[test]
fn a_real_digest_lands_over_the_seeded_notes_live() {
    let Some((client, ledger)) = live("live-a4-digest") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();

    fixture::runtime().block_on(|| async {
        let (engine, account, app) =
            armed_app(&client, &store, &host, &port, &user, &password).await;
        let notes = categorise_the_notes(&app, &store, &account).await;

        // The textless-2xx retry: a failed generation stores nothing, so a
        // bounded retry is honest and the store keeps the truth.
        let mut view = None;
        for _ in 0..3 {
            if let Ok(generated) = app.generate_digest(&account, DAYS).await {
                view = Some(generated);
                break;
            }
        }
        let view = view.expect("the digest answered within three tries");
        assert!(
            !view.content.trim().is_empty(),
            "the digest is real text, not a blank"
        );
        assert!(
            !view.regenerated,
            "the first generation over this period replaced nothing"
        );

        // The honest window: the whole UTC days the clamp pins, and the
        // notes really inside it, so the rollup counted them.
        assert_eq!(
            window_bounds(DAYS, view.period_end),
            (view.period_start, view.period_end),
            "the stored period is the quantised {DAYS}-day window of its own end",
        );
        let start = civil_unix(view.period_start).expect("the period start holds");
        let end = civil_unix(view.period_end).expect("the period end holds");
        for note in &notes {
            let stored = engine
                .messages_by_keys(&note.account, std::slice::from_ref(&note.key))
                .await
                .expect("the note resolves");
            let date = civil_of(stored.first().expect("the stored copy"))
                .expect("the seeded note carries its date");
            assert!(
                start <= date && date <= end,
                "a seeded note falls outside the window the digest rode: {date:?}"
            );
        }

        // The stored row and its cost.
        let latest = app
            .latest_document(&account, AiDocumentKind::Digest)
            .expect("the stored digest answers");
        assert_eq!(latest.content, view.content);
        assert_eq!(latest.period_start, view.period_start);
        assert_eq!(latest.period_end, view.period_end);
        assert!(
            ledger.rows().iter().any(|row| row.use_case == "digest"
                && row.tier == "default"
                && row.outcome == "ok"
                && row.prompt_tokens > 0
                && row.prompt_tokens < 100_000
                && row.completion_tokens > 0
                && !row.model.is_empty()),
            "the digest spent its default-tier ledger row with sane tokens"
        );
    });
}

/// A real report over the same window: the three sections parse out of
/// the assembled document, each carrying real text (the structured
/// shape's own proof), and the stored row lands under the same quantised
/// window with the notes inside it. The statistics the report grounds
/// itself in are computed in code before the call; their arithmetic is
/// pinned offline, and the live proof pins the window they rode, so the
/// counts counted real mail.
#[test]
fn a_real_report_parses_three_sections_live() {
    let Some((client, ledger)) = live("live-a4-report") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();

    fixture::runtime().block_on(|| async {
        let (engine, account, app) =
            armed_app(&client, &store, &host, &port, &user, &password).await;
        categorise_the_notes(&app, &store, &account).await;

        let mut view = None;
        for _ in 0..3 {
            if let Ok(generated) = app.generate_report(&account, DAYS).await {
                view = Some(generated);
                break;
            }
        }
        let view = view.expect("the report answered within three tries");
        for (heading, next) in [
            ("Executive summary", Some("Response patterns")),
            ("Response patterns", Some("Recommendations")),
            ("Recommendations", None),
        ] {
            let (_, rest) = view
                .content
                .split_once(heading)
                .unwrap_or_else(|| panic!("the {heading} heading is absent:\n{}", view.content));
            let section = next
                .and_then(|next| rest.split_once(next))
                .map_or(rest, |(section, _)| section);
            assert!(
                !section.trim().is_empty(),
                "the {heading} section carries real text:\n{}",
                view.content
            );
        }

        assert_eq!(
            window_bounds(DAYS, view.period_end),
            (view.period_start, view.period_end),
            "the stored period is the quantised {DAYS}-day window of its own end",
        );
        let start = civil_unix(view.period_start).expect("the period start holds");
        let end = civil_unix(view.period_end).expect("the period end holds");
        let inside = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .iter()
            .filter(|message| civil_of(message).is_some_and(|date| start <= date && date <= end))
            .count();
        assert!(
            inside >= NOTES.len(),
            "the window the report's statistics rode holds the seeded notes: {inside} inside"
        );

        let latest = app
            .latest_document(&account, AiDocumentKind::Report)
            .expect("the stored report answers");
        assert_eq!(latest.content, view.content);
        assert!(
            ledger.rows().iter().any(|row| row.use_case == "report"
                && row.tier == "default"
                && row.outcome == "ok"
                && row.prompt_tokens > 0
                && row.completion_tokens > 0),
            "the report spent its default-tier ledger row"
        );
    });
}

/// The split map over the pass's real results: every decided
/// categorisation the pass landed serves its pinned side, every key is a
/// message the engine really holds, and the derivation spends no model
/// call at all (the ledger row count the no-call proof rides, the
/// zero-model-calls boundary the design pins).
#[test]
fn the_split_map_serves_over_the_pass_s_real_results_live() {
    let Some((client, ledger)) = live("live-a4-split") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();

    fixture::runtime().block_on(|| async {
        let (engine, account, app) =
            armed_app(&client, &store, &host, &port, &user, &password).await;
        categorise_the_notes(&app, &store, &account).await;

        let before = ledger.rows().len();
        let entries = app.split_map(&account).await;
        assert_eq!(
            ledger.rows().len(),
            before,
            "the split map spent no model call at all"
        );
        assert!(
            !entries.is_empty(),
            "the pass's decided categorisation serves a non-empty map"
        );

        let held: Vec<String> = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .into_iter()
            .map(|message| message.id.key().as_str().to_owned())
            .collect();
        for entry in &entries {
            assert!(
                held.contains(&entry.key),
                "every mapped key is a message the engine holds: {entry:?}"
            );
            let word = store
                .ai_results_for_message(account.as_str(), &entry.key)
                .expect("the results read")
                .iter()
                .find(|row| row.stage == "categorise")
                .and_then(|row| row.category.clone())
                .unwrap_or_else(|| panic!("a mapped key holds a decided word: {entry:?}"));
            assert_eq!(
                side_for(&word),
                Some(entry.side),
                "the pinned mapping assigns the stored word its side: {word}"
            );
        }
    });
}
