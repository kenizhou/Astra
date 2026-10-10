// SPDX-License-Identifier: GPL-3.0-only
//! `ai cold` and `ai results` tests: the stored-verdict reads over the
//! results store the incoming pass fills (migration 0003), never a live
//! call. The unjudged answer is null, not a guess; every row the pass
//! recorded lists back; and the account index check the CLI's contract
//! carries holds for the positional grammar too.

use astra_ai::{AiStore, CorrelationId, NewAiResult};

use super::fakes::{temp_dir, write_index};
use super::run;

/// One command line as the `run` layer takes it.
fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// One results row to seed, in the pass's own vocabulary.
struct Row {
    stage: &'static str,
    rule_id: Option<&'static str>,
    verdict: &'static str,
    category: Option<&'static str>,
}

/// Seeds `rows` for message `key` of account `alpha`, oldest first, each
/// stamped with its own correlation id the way separate passes would.
fn seeded(rows: &[Row], key: &str) -> std::path::PathBuf {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let store = AiStore::open(&dir.join(super::AI_STORE_FILE)).unwrap();
    for (index, row) in rows.iter().enumerate() {
        store
            .record_ai_result(&NewAiResult {
                correlation: CorrelationId::new(format!("turn-{}", index + 1)),
                message_account: "alpha".to_owned(),
                message_key: key.to_owned(),
                stage: row.stage.to_owned(),
                rule_id: row.rule_id.map(|id| astra_ai::RuleId::new(id.to_owned())),
                verdict: row.verdict.to_owned(),
                category: row.category.map(str::to_owned),
            })
            .unwrap();
    }
    dir
}

#[test]
fn ai_cold_answers_the_stored_verdict_and_null_for_unjudged() {
    let dir = seeded(
        &[
            Row {
                stage: "categorise",
                rule_id: None,
                verdict: "ok",
                category: Some("newsletter"),
            },
            Row {
                stage: "cold",
                rule_id: None,
                verdict: "no",
                category: None,
            },
        ],
        "m1",
    );
    let outcome = run(
        &line(&["ai", "cold", "alpha", "m1", "m2"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let messages = outcome.envelope.data().unwrap()["messages"].clone();
    assert_eq!(messages.as_array().unwrap().len(), 2);
    assert_eq!(messages[0]["key"], "m1");
    let cold = &messages[0]["cold"];
    assert_eq!(cold["verdict"], "no");
    assert_eq!(cold["correlation"], "turn-2");
    assert!(cold["recorded_at"].as_i64().unwrap() > 0);
    assert!(
        messages[1]["cold"].is_null(),
        "an unjudged message answers null, not a guess: {messages}"
    );
}

#[test]
fn ai_cold_answers_the_newest_cold_row() {
    let dir = seeded(
        &[
            Row {
                stage: "cold",
                rule_id: None,
                verdict: "abstain_default",
                category: None,
            },
            Row {
                stage: "cold",
                rule_id: None,
                verdict: "yes",
                category: None,
            },
        ],
        "m1",
    );
    let outcome = run(
        &line(&["ai", "cold", "alpha", "m1"]),
        &dir,
        &mut std::io::empty(),
    );
    let cold = &outcome.envelope.data().unwrap()["messages"][0]["cold"];
    assert_eq!(cold["verdict"], "yes", "the newest pass's word stands");
    assert_eq!(cold["correlation"], "turn-2");
}

#[test]
fn ai_results_lists_every_stage_row_newest_first() {
    let dir = seeded(
        &[
            Row {
                stage: "rule_match",
                rule_id: Some("rule-1"),
                verdict: "yes",
                category: None,
            },
            Row {
                stage: "categorise",
                rule_id: None,
                verdict: "ok",
                category: Some("newsletter"),
            },
            Row {
                stage: "cold",
                rule_id: None,
                verdict: "no",
                category: None,
            },
        ],
        "m1",
    );
    let outcome = run(
        &line(&["ai", "results", "alpha", "m1"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let results = outcome.envelope.data().unwrap()["messages"][0]["results"].clone();
    let stages: Vec<&str> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["stage"].as_str().unwrap())
        .collect();
    assert_eq!(stages, vec!["cold", "categorise", "rule_match"]);
    assert_eq!(results[2]["rule_id"], "rule-1");
    assert_eq!(results[2]["verdict"], "yes");
    assert_eq!(results[1]["category"], "newsletter");
    assert_eq!(results[1]["rule_id"], serde_json::Value::Null);
    assert_eq!(results[0]["correlation"], "turn-3");
}

#[test]
fn ai_verbs_want_a_key_and_a_known_account() {
    let dir = seeded(&[], "m1");
    let no_key = run(&line(&["ai", "cold", "alpha"]), &dir, &mut std::io::empty());
    assert_eq!(no_key.code.exit(), 2);
    let text = no_key.envelope.render(false).unwrap();
    assert!(text.contains("at least one key"), "{text}");

    let ghost = run(
        &line(&["ai", "results", "ghost", "m1"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(ghost.code.exit(), 1);
    let text = ghost.envelope.render(false).unwrap();
    assert!(text.contains("unknown account ghost"), "{text}");
}

#[test]
fn ai_invocations_land_their_audit_rows() {
    let dir = seeded(
        &[Row {
            stage: "cold",
            rule_id: None,
            verdict: "no",
            category: None,
        }],
        "m1",
    );
    run(
        &line(&[
            "ai",
            "cold",
            "alpha",
            "m1",
            "m2",
            "--correlation-id",
            "turn-9",
        ]),
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
            "ai cold".to_owned(),
            "account=alpha keys=2".to_owned(),
            "ok".to_owned()
        ),
        "the caller-typed keys never reach the ledger, only their count"
    );
}
