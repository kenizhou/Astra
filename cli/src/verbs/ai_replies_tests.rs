// SPDX-License-Identifier: GPL-3.0-only
//! The reply-drafting `ai` verbs' tests: `ai drafts`, `ai summary`, and
//! `ai style` read the pool, cache, and profile tables (migrations 0004
//! and 0005) the app's drafting driver and pulls fill, never a live
//! call; `ai style rebuild` refuses under the model rule and lands its
//! refused audit row; and the grammar pins its operands.

use astra_ai::{AiStore, CorrelationId, NewDraftPoolRow};

use super::fakes::{temp_dir, write_index};
use super::run;

/// One command line as the `run` layer takes it.
fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// Seeds the reply-drafting stores for account `alpha`: one drafted pool
/// row then one skipped row for `m1` (same-second inserts, so the store's
/// id ordering lists the skipped row newest), a cached summary for `m1`,
/// and a style profile.
fn replies_dir() -> std::path::PathBuf {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let store = AiStore::open(&dir.join(super::AI_STORE_FILE)).unwrap();
    for (turn, (reply, confidence)) in [
        (
            Some("Tuesday works for the launch review.".to_owned()),
            Some(0.8),
        ),
        (None, None),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .insert_draft_pool(&NewDraftPoolRow {
                correlation: CorrelationId::new(format!("turn-{}", turn + 1)),
                message_account: "alpha".to_owned(),
                message_key: "m1".to_owned(),
                reply,
                confidence,
            })
            .unwrap();
    }
    store
        .cache_summary("alpha", "m1", "hash-a", "Ada asks for the launch date.")
        .unwrap();
    store
        .save_style_profile("alpha", "Warm and brief; signs off with thanks.")
        .unwrap();
    dir
}

#[test]
fn ai_drafts_lists_pool_rows_newest_first_with_the_skipped_flag() {
    let dir = replies_dir();
    let outcome = run(
        &line(&["ai", "drafts", "alpha", "m1", "m2"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let messages = outcome.envelope.data().unwrap()["messages"].clone();
    assert_eq!(messages.as_array().unwrap().len(), 2);
    let drafts = &messages[0]["drafts"];
    assert_eq!(
        drafts.as_array().unwrap().len(),
        2,
        "every generation turn the pool recorded lists back"
    );
    assert_eq!(drafts[0]["correlation"], "turn-2");
    assert_eq!(drafts[0]["skipped"], true, "the newest row was a failure");
    assert_eq!(drafts[0]["reply"], serde_json::Value::Null);
    assert_eq!(drafts[0]["confidence"], serde_json::Value::Null);
    assert!(drafts[0]["created_at"].as_i64().unwrap() > 0);
    assert_eq!(drafts[1]["correlation"], "turn-1");
    assert_eq!(drafts[1]["skipped"], false);
    assert_eq!(drafts[1]["reply"], "Tuesday works for the launch review.");
    assert_eq!(drafts[1]["confidence"], 0.8);
    assert_eq!(
        messages[1]["drafts"],
        serde_json::json!([]),
        "a message no generation touched answers an empty list"
    );
}

#[test]
fn ai_summary_answers_the_cached_row_and_null_when_nothing_is_cached() {
    let dir = replies_dir();
    let outcome = run(
        &line(&["ai", "summary", "alpha", "m1", "m2"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let messages = outcome.envelope.data().unwrap()["messages"].clone();
    assert_eq!(messages.as_array().unwrap().len(), 2);
    let summary = &messages[0]["summary"];
    assert_eq!(summary["summary"], "Ada asks for the launch date.");
    assert_eq!(summary["body_hash"], "hash-a");
    assert!(summary["created_at"].as_i64().unwrap() > 0);
    assert!(
        messages[1]["summary"].is_null(),
        "an unsummarised message answers null: {messages}"
    );
}

#[test]
fn ai_style_answers_the_stored_profile_and_null_without_one() {
    let dir = replies_dir();
    let outcome = run(
        &line(&["ai", "style", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let style = &outcome.envelope.data().unwrap()["style"];
    assert_eq!(style["note"], "Warm and brief; signs off with thanks.");
    assert!(style["built_at"].as_i64().unwrap() > 0);

    let bare = temp_dir();
    write_index(&bare, &["alpha"]);
    let outcome = run(
        &line(&["ai", "style", "alpha"]),
        &bare,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(
        outcome.envelope.data().unwrap()["style"].is_null(),
        "an account whose profile was never built answers null"
    );
}

#[test]
fn ai_style_rebuild_refuses_under_the_model_rule() {
    let dir = replies_dir();
    let outcome = run(
        &line(&["ai", "style", "rebuild"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 3, "the refusal is its own exit code");
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("dials no model"), "{text}");
    assert!(text.contains("the app"), "{text}");
    assert!(text.contains("ai style"), "{text}");
}

#[test]
fn ai_style_rebuild_ships_no_preview_because_nothing_can_run() {
    let dir = replies_dir();
    let previewed = run(
        &line(&["ai", "style", "rebuild", "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(previewed.code.exit(), 2);
    let text = previewed.envelope.render(false).unwrap();
    assert!(
        text.contains("only the mutating verbs ship --preview"),
        "{text}"
    );
    let with_operands = run(
        &line(&["ai", "style", "rebuild", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(with_operands.code.exit(), 2);
    let text = with_operands.envelope.render(false).unwrap();
    assert!(text.contains("no further words"), "{text}");
}

#[test]
fn ai_style_wants_one_account_and_no_keys() {
    let dir = replies_dir();
    let bare = run(&line(&["ai", "style"]), &dir, &mut std::io::empty());
    assert_eq!(bare.code.exit(), 2);
    let text = bare.envelope.render(false).unwrap();
    assert!(text.contains("ai style wants an account"), "{text}");

    let extra = run(
        &line(&["ai", "style", "alpha", "m1"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(extra.code.exit(), 2);
    let text = extra.envelope.render(false).unwrap();
    assert!(text.contains("one account"), "{text}");
}

#[test]
fn ai_replies_verbs_want_keys_and_a_known_account() {
    let dir = replies_dir();
    let no_key = run(
        &line(&["ai", "drafts", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(no_key.code.exit(), 2);
    let text = no_key.envelope.render(false).unwrap();
    assert!(text.contains("at least one key"), "{text}");

    let ghost = run(
        &line(&["ai", "summary", "ghost", "m1"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(ghost.code.exit(), 1);
    let text = ghost.envelope.render(false).unwrap();
    assert!(text.contains("unknown account ghost"), "{text}");

    let ghost_style = run(
        &line(&["ai", "style", "ghost"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(ghost_style.code.exit(), 1);
    let text = ghost_style.envelope.render(false).unwrap();
    assert!(text.contains("unknown account ghost"), "{text}");
}

#[test]
fn ai_replies_invocations_land_their_audit_rows() {
    let dir = replies_dir();
    for (line_words, verb, preview) in [
        (
            line(&["ai", "drafts", "alpha", "m1", "m2"]),
            "ai drafts",
            "account=alpha keys=2".to_owned(),
        ),
        (
            line(&["ai", "summary", "alpha", "m1", "m2"]),
            "ai summary",
            "account=alpha keys=2".to_owned(),
        ),
        (
            line(&["ai", "style", "alpha"]),
            "ai style",
            "account=alpha".to_owned(),
        ),
    ] {
        let correlation = format!("turn-{}", verb.replace(' ', "-"));
        let mut words = line_words;
        words.push("--correlation-id".to_owned());
        words.push(correlation.clone());
        run(&words, &dir, &mut std::io::empty());
        let conn = rusqlite::Connection::open(dir.join(super::AI_STORE_FILE)).unwrap();
        let row: (String, String, String) = conn
            .query_row(
                "SELECT verb, arg_preview, outcome FROM audit_log
                 WHERE correlation_id = ?1",
                [&correlation],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (verb.to_owned(), preview, "ok".to_owned()));
    }
}

#[test]
fn the_refused_rebuild_lands_its_audit_row() {
    let dir = replies_dir();
    run(
        &line(&["ai", "style", "rebuild", "--correlation-id", "turn-9"]),
        &dir,
        &mut std::io::empty(),
    );
    let conn = rusqlite::Connection::open(dir.join(super::AI_STORE_FILE)).unwrap();
    let row: (String, String, String) = conn
        .query_row(
            "SELECT verb, arg_preview, outcome FROM audit_log
             WHERE correlation_id = 'turn-9'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "ai style rebuild".to_owned(),
            String::new(),
            "refused".to_owned()
        ),
        "the ledger names the refused spelling and its outcome"
    );
}
