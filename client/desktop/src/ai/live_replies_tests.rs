// SPDX-License-Identifier: GPL-3.0-only
//! The gated live AI replies suite: the A2 stages proven against the real
//! model endpoint, over the host's own seam implementations. A real draft
//! lands in the pool for the seeded rich note with its ledger cost and the
//! thread-history caps sanity-checked against the real thread, the chips
//! pull reads the surfacing gate end to end over that real pool, a pinned
//! useful reply earns a real JudgeDraftSurface verdict, the summarise
//! action answers twice on one network call (the ledger row count is the
//! proof), and the style profile is inferred live from astra1's Sent mail
//! (the proof's own send files the copy) with the Sent window
//! sanity-checked and the daily cap spending no second call.
//!
//! The gates are the A1 suite's: `ASTRA_LIVE_LLM=1` for the model
//! connection, plus `ASTRA_LIVE_HARNESS=1` for the tests that need the
//! local Stalwart harness; an open gate with an unusable connection fails
//! loudly rather than skipping. The recorded invocation runs this suite
//! one test at a time (`-- --nocapture --test-threads=1`): the tests share
//! one model endpoint and one live server, and serial calls keep a rate
//! limit from failing a neighbour.
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! export ASTRA_LIVE_LLM=1
//! export ASTRA_LLM_DEFAULT=anthropic:deepseek-flash
//! export ASTRA_LLM_API_KEY_ANTHROPIC=<the key>
//! export ASTRA_LLM_BASE_URL_ANTHROPIC=https://api.deepseek.com/anthropic
//! cargo test -p astra-desktop ai::live_replies -- --nocapture --test-threads=1
//! ```

// The harness-backed support: its own file so this one stays under the
// 500-line cap the gate enforces. The explicit path because this file is
// loaded through `#[path]` and so owns no directory a bare `mod` would
// resolve into.
#[path = "live_replies/harness.rs"]
mod harness;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use astra_ai::{CorrelationId, DlpPolicy, NewDraftPoolRow};
use astra_app::{
    chips_for, DraftConfidence, Intent, CHIP_CONFIDENCE_FLOOR, CHIP_LIMIT, CHIP_PREVIEW_CAP,
    STYLE_SENT_WINDOW, THREAD_BODY_CAP, THREAD_HISTORY_LIMIT,
};
use astra_viewmodel::FolderRole;
use engine_api::{AccountId, Engine, ProviderKey};

use super::live_tests::live;
use super::{incoming_pass, LlmAiJudge};
use crate::ai::fixture;

use harness::{
    delete_subject_mail, dial, document, harness, replies_app, row_by_subject, select_projects,
    select_role_folder, STYLE_SUBJECT, STYLE_WORDS,
};

/// How long the suite waits on a spawned driver or a delivery: the A1
/// suite's five minute bound.
const DEADLINE: Duration = Duration::from_secs(300);

/// The reply the gate fixtures pin: grounded, complete, and useful, the
/// drafted reply a reader could send as it stands.
const USEFUL_REPLY: &str = "Yes, the prototype still lands next Friday, and the review notes \
                            will follow the week after.";

/// A real draft for the seeded rich note: the open arms the driver, the
/// driver the host runs generates over the engine's own thread and body
/// reads, and the landed pool row carries a drafted reply with its
/// confidence inside the closed set beside its ledger cost. The
/// thread-history caps get their live sanity check against the real thread
/// (the collector's mail budget stays bounded), and the chips pull then
/// reads the surfacing gate end to end over the real pool: at most the
/// chip pair, previews capped, a judge verdict spent exactly when the
/// draft cleared the confidence floor. The open marked the note read, so
/// the run puts it back the way the seed left it.
#[test]
fn a_real_draft_lands_for_the_seeded_rich_note_live() {
    let Some((client, ledger)) = live("live-a2-draft") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();
    let ai = incoming_pass(
        Some(Arc::clone(&client)),
        Arc::clone(&store),
        Ok(DlpPolicy::Allow),
        None,
    )
    .expect("the pass arms over the live client");

    fixture::runtime().block_on(|| async {
        let (account, providers) = dial(&host, &port, &user, &password).await;
        let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
        let app = replies_app(Arc::clone(&engine), account.clone(), providers, Some(ai));
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the refresh");
        select_projects(&app, &account).await;
        let message = row_by_subject(&app, "Harness rich note");

        // The thread as the collector reads it: the member count beside the
        // caps' budget is the live sanity that one real thread stays inside
        // the drafting prompt's mail ceiling. A threadless original is a
        // legitimate shape (the collector drafts from it alone), so the
        // thread read rides only when the engine threaded the note.
        let stored = engine
            .messages_by_keys(&account, std::slice::from_ref(&message.key))
            .await
            .expect("the rich note resolves");
        let thread = stored
            .first()
            .expect("the stored copy")
            .thread_id()
            .cloned();
        let members = match thread.as_ref() {
            Some(thread) => engine
                .mail_on_threads(std::slice::from_ref(&account), [thread.as_str()])
                .await
                .expect("the thread read"),
            None => Vec::new(),
        };
        assert!(
            members.len() <= THREAD_HISTORY_LIMIT + 1,
            "the real thread fits the collector's history window: {} members",
            members.len()
        );
        let budget = (THREAD_HISTORY_LIMIT + 1).saturating_mul(THREAD_BODY_CAP + 32);
        assert!(
            budget <= 40_000,
            "the thread-history caps keep one draft prompt's mail budget bounded: {budget}"
        );

        let driver = tokio::spawn({
            let app = Arc::clone(&app);
            async move { app.drive_reply_drafting().await }
        });
        app.dispatch(Intent::OpenMessage {
            message: message.clone(),
        })
        .await
        .expect("the open");

        let deadline = Instant::now() + DEADLINE;
        let rows = loop {
            let rows = store
                .draft_pool_for_message(account.as_str(), message.key.as_str())
                .expect("the pool read");
            if let Some(row) = rows.first() {
                assert!(
                    row.reply
                        .as_deref()
                        .is_some_and(|reply| !reply.trim().is_empty()),
                    "the landed row is a drafted reply, not a skipped one: {row:?}"
                );
                assert!(
                    row.confidence.is_some_and(|score| {
                        DraftConfidence::ALL
                            .iter()
                            .any(|word| (word.score() - score).abs() < 1e-9)
                    }),
                    "the drafted confidence is inside the closed set: {row:?}"
                );
                break rows;
            }
            assert!(
                Instant::now() < deadline,
                "the driver never drafted the rich note within five minutes"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        driver.abort();
        let landed = rows.first().expect("the drafted row");

        let correlation = landed.correlation.as_str();
        let ledger_rows = ledger.rows();
        let attempts: Vec<_> = ledger_rows
            .iter()
            .filter(|row| row.use_case == "draft_reply" && row.correlation == correlation)
            .collect();
        assert!(
            attempts.iter().any(|row| row.outcome == "ok"
                && row.prompt_tokens > 0
                && row.prompt_tokens < 100_000
                && row.completion_tokens > 0
                && !row.model.is_empty()),
            "the generation landed a sane ledger cost under the open's correlation: {attempts:?}"
        );

        // The chips pull over the real pool: the host command's own path.
        let chips = app.draft_chips(&account, &message.key).await;
        assert!(
            chips.len() <= CHIP_LIMIT,
            "the pull surfaces at most the chip pair: {chips:?}"
        );
        for chip in &chips {
            assert!(
                !chip.reply.trim().is_empty(),
                "a surfaced chip carries its whole reply"
            );
            assert!(
                chip.preview.chars().count() <= CHIP_PREVIEW_CAP + 3,
                "the preview stays inside the one-line budget: {:?}",
                chip.preview
            );
        }
        if let Some(reply) = chips.first() {
            assert_eq!(
                Some(reply.reply.trim()),
                landed.reply.as_deref().map(str::trim),
                "a surfaced chip carries the pooled draft the gate cleared"
            );
        }
        let verdicts = ledger
            .rows()
            .into_iter()
            .filter(|row| row.use_case == "judge_draft_surface")
            .count();
        let cleared = landed.confidence.unwrap_or_default() >= CHIP_CONFIDENCE_FLOOR;
        assert_eq!(
            verdicts > 0,
            cleared,
            "a judge verdict is spent exactly when the draft cleared the confidence floor"
        );

        // Put the note back the way the seed left it.
        app.dispatch(Intent::MarkRead {
            row: astra_app::RowRef::Message(message),
            read: false,
        })
        .await
        .expect("the unread reset");
    });
}

/// A real gate decision: a pooled high-confidence reply the reader could
/// send as it stands, judged through the host's own judge seam. The verdict
/// lands in the ledger (nano tier, the judge outcome word) and the chip
/// surfaces, the gate's positive pole. No harness mail is touched: the pool
/// row is the fixture.
#[test]
fn the_gate_judge_decides_a_useful_draft_live() {
    let Some((client, ledger)) = live("live-a2-gate") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let store = ledger.store();
    let account = AccountId::try_from("work").expect("the account id");
    let key = ProviderKey::new("m-live-gate").expect("the key");
    let correlation = CorrelationId::new("corr-live-a2-gate".to_string());
    store
        .insert_draft_pool(&NewDraftPoolRow {
            correlation: correlation.clone(),
            message_account: account.as_str().to_owned(),
            message_key: key.as_str().to_owned(),
            reply: Some(USEFUL_REPLY.to_owned()),
            confidence: Some(DraftConfidence::High.score()),
        })
        .expect("the pooled fixture row");

    let seam = LlmAiJudge::new(client);
    let chips =
        fixture::runtime().block_on(|| chips_for(&account, &key, &store, &seam, &correlation));
    assert_eq!(
        chips.len(),
        1,
        "the useful draft surfaces its chip: {chips:?}"
    );
    assert_eq!(chips[0].reply, USEFUL_REPLY);

    let rows = ledger.rows();
    assert!(
        rows.iter().any(|row| row.correlation == "corr-live-a2-gate"
            && row.use_case == "judge_draft_surface"
            && row.tier == "nano"
            && row.outcome.starts_with("judge:")),
        "the gate verdict landed its ledger row: {rows:?}"
    );
}

/// A real summarise, cached: the first call answers from the model and the
/// second answers identical text with no new ledger row, the count the
/// no-network-request proof rides. The seeded plain note is the body.
#[test]
fn summarise_answers_twice_on_one_live_call() {
    let Some((client, ledger)) = live("live-a2-summarise") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();
    let ai = incoming_pass(
        Some(Arc::clone(&client)),
        Arc::clone(&store),
        Ok(DlpPolicy::Allow),
        None,
    )
    .expect("the pass arms over the live client");
    let summarise_rows = || {
        ledger
            .rows()
            .into_iter()
            .filter(|row| row.use_case == "summarise")
            .count()
    };

    fixture::runtime().block_on(|| async {
        let (account, providers) = dial(&host, &port, &user, &password).await;
        let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
        let app = replies_app(engine, account.clone(), providers, Some(ai));
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the refresh");
        select_role_folder(&app, &account, FolderRole::Inbox).await;
        let message = row_by_subject(&app, "Harness note one");

        let before = summarise_rows();
        // The textless-2xx retry: the cache stores nothing on failure, so a
        // bounded retry is honest and the row count measures real spending.
        let mut first = None;
        for _ in 0..3 {
            if let Ok(summary) = app.summarise_message(&account, &message.key).await {
                first = Some(summary);
                break;
            }
        }
        let first = first.expect("the summarise answered within three tries");
        let spent = summarise_rows();
        assert!(spent > before, "the first summarise spent its model call");
        assert!(
            !first.trim().is_empty(),
            "the summary is real text, not a blank"
        );

        let second = app
            .summarise_message(&account, &message.key)
            .await
            .expect("the second summarise answers");
        assert_eq!(
            summarise_rows(),
            spent,
            "the cached second call made no network request"
        );
        assert_eq!(second, first, "the cached answer is the first answer");
    });
}

/// A real style infer from Sent mail: the proof sends its own mail through
/// the product's submit path (which files the sender's copy in Sent, the
/// record the spec's post-send copies sentence meant), the Sent window
/// sanity check confirms the window covers the account's whole synced set,
/// the pull infers the note through the live seam and stores the profile,
/// and the daily cap keeps a second pull from spending another call. The
/// run withdraws its own copies afterwards through the engine's delete
/// path, so the harness keeps its seeded shape.
#[test]
fn the_style_profile_is_inferred_live_from_sent_mail() {
    let Some((client, ledger)) = live("live-a2-style") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();
    let ai = incoming_pass(
        Some(Arc::clone(&client)),
        Arc::clone(&store),
        Ok(DlpPolicy::Allow),
        None,
    )
    .expect("the pass arms over the live client");
    let infer_rows = || {
        ledger
            .rows()
            .into_iter()
            .filter(|row| row.use_case == "style_infer")
            .count()
    };

    fixture::runtime().block_on(|| async {
        let (account, providers) = dial(&host, &port, &user, &password).await;
        let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
        let app = replies_app(Arc::clone(&engine), account.clone(), providers, Some(ai));
        // A second dial's first provider is the sweep's own handle: the app
        // keeps its providers private, and the cleanup needs one the test
        // holds.
        let (_scratch_account, cleanup_providers) = dial(&host, &port, &user, &password).await;
        let cleanup = cleanup_providers.into_iter().next().expect("a provider");

        // Back to a known state: a run interrupted before its cleanup may
        // have left the proof's copies behind, delivered or filed.
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the sweep refresh");
        delete_subject_mail(&engine, &cleanup, &account, &[STYLE_SUBJECT]).await;
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the reset refresh");
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the observed refresh");

        app.dispatch(Intent::SubmitRichMail {
            from: account.clone(),
            to: "astra1@example.test".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: STYLE_SUBJECT.to_owned(),
            document: document(STYLE_WORDS),
            blobs: Vec::new(),
        })
        .await
        .expect("the proof send");
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the filed refresh");

        // The filed copy the infer reads: the Sent folder holds it.
        select_role_folder(&app, &account, FolderRole::Sent).await;
        let filed = row_by_subject(&app, STYLE_SUBJECT);
        assert_eq!(filed.account, account, "the filed copy is the sender's own");

        // The Sent window sanity: the whole synced set fits inside the
        // window the collector draws, so no Sent mail hides behind it.
        let total = engine
            .messages(&account)
            .await
            .expect("the account's stored messages")
            .len();
        assert!(
            total <= STYLE_SENT_WINDOW,
            "the Sent window covers the account's whole synced set: {total} messages"
        );

        let before = infer_rows();
        // The same retry: a failed infer stores nothing.
        let mut note = None;
        for _ in 0..3 {
            if let Some(pulled) = app.account_style_note(&account).await {
                note = Some(pulled);
                break;
            }
        }
        let note = note.expect("the style pull answered within three tries");
        assert!(
            !note.trim().is_empty(),
            "the inferred note is real text, not a blank"
        );
        let spent = infer_rows();
        assert!(
            spent > before,
            "the infer spent its model call: {} rows",
            spent - before
        );
        assert!(
            ledger.rows().iter().any(|row| row.use_case == "style_infer"
                && row.tier == "economy"
                && row.outcome == "ok"
                && row.prompt_tokens > 0
                && row.prompt_tokens < 100_000),
            "the infer landed its economy-tier ledger row with sane tokens"
        );
        assert!(
            store
                .style_profile(account.as_str())
                .expect("the profile read")
                .is_some(),
            "the inferred note was stored as the account's profile"
        );

        // The daily cap: a second pull spends nothing and answers the same.
        let again = app
            .account_style_note(&account)
            .await
            .expect("the second style pull answers");
        assert_eq!(infer_rows(), spent, "the daily cap spent no second infer");
        assert_eq!(again, note, "the capped pull answers the stored note");

        // The cleanup: both copies withdrawn, the harness seeded again.
        delete_subject_mail(&engine, &cleanup, &account, &[STYLE_SUBJECT]).await;
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the cleanup refresh");
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the observed cleanup");
    });
}
