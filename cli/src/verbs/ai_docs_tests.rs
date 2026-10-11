// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents `ai` verbs' tests: `ai digest` and `ai report` read
//! the documents store (migration 0010) the app's generation pulls fill,
//! never a live call, answering the account's latest stored document of
//! their own kind with its period, the store's stamp, the generation
//! turn's correlation, and the whole content; `ai digest generate` and
//! `ai report generate` refuse under the model rule and land their
//! refused audit rows; and the grammar pins its operands.

use astra_ai::{AiDocumentKind, AiStore, CorrelationId, NewAiDocument};

use super::fakes::{temp_dir, write_index};
use super::run;

/// One command line as the `run` layer takes it.
fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// Seeds account `alpha`'s documents store: an older digest period, the
/// newer digest the latest read must answer (same-second puts, so the
/// store's id ordering decides), and a report over the newer period the
/// digest verb must not answer.
fn docs_dir() -> std::path::PathBuf {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let store = AiStore::open(&dir.join(super::AI_STORE_FILE)).unwrap();
    for (kind, period, content, correlation) in [
        (
            AiDocumentKind::Digest,
            (1, 2),
            "An older week.".to_owned(),
            "turn-1",
        ),
        (
            AiDocumentKind::Digest,
            (3, 4),
            "Two replies wait; the newsletter can wait.".to_owned(),
            "turn-2",
        ),
        (
            AiDocumentKind::Report,
            (3, 4),
            "Executive summary\nA launch week.".to_owned(),
            "turn-3",
        ),
    ] {
        store
            .put_document(&NewAiDocument {
                account: "alpha".to_owned(),
                kind,
                period_start: period.0,
                period_end: period.1,
                content,
                correlation: CorrelationId::new(correlation.to_owned()),
            })
            .unwrap();
    }
    dir
}

#[test]
fn ai_digest_answers_the_latest_stored_document_and_null_without_one() {
    let dir = docs_dir();
    let outcome = run(
        &line(&["ai", "digest", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let digest = &outcome.envelope.data().unwrap()["digest"];
    assert_eq!(digest["period_start"], 3);
    assert_eq!(digest["period_end"], 4);
    assert_eq!(
        digest["content"],
        "Two replies wait; the newsletter can wait."
    );
    assert_eq!(digest["correlation"], "turn-2");
    assert!(
        digest["created_at"].as_i64().unwrap() > 0,
        "the store's stamp answers: {digest}"
    );

    let bare = temp_dir();
    write_index(&bare, &["alpha"]);
    let outcome = run(
        &line(&["ai", "digest", "alpha"]),
        &bare,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(
        outcome.envelope.data().unwrap()["digest"].is_null(),
        "an account no generation touched answers null, not a guess"
    );
}

#[test]
fn ai_report_answers_its_own_kind_alone() {
    let dir = docs_dir();
    let outcome = run(
        &line(&["ai", "report", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let report = &outcome.envelope.data().unwrap()["report"];
    assert_eq!(report["content"], "Executive summary\nA launch week.");
    assert_eq!(report["correlation"], "turn-3");

    // A digest alone answers no report: the kinds never bleed.
    let digest_only = temp_dir();
    write_index(&digest_only, &["alpha"]);
    let store = AiStore::open(&digest_only.join(super::AI_STORE_FILE)).unwrap();
    store
        .put_document(&NewAiDocument {
            account: "alpha".to_owned(),
            kind: AiDocumentKind::Digest,
            period_start: 3,
            period_end: 4,
            content: "Two replies wait.".to_owned(),
            correlation: CorrelationId::new("turn-4".to_owned()),
        })
        .unwrap();
    let outcome = run(
        &line(&["ai", "report", "alpha"]),
        &digest_only,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(
        outcome.envelope.data().unwrap()["report"].is_null(),
        "the report verb answers the report rows alone"
    );
}

#[test]
fn ai_document_generations_refuse_under_the_model_rule() {
    let dir = docs_dir();
    for word in ["digest", "report"] {
        let refused = run(
            &line(&["ai", word, "generate"]),
            &dir,
            &mut std::io::empty(),
        );
        assert_eq!(
            refused.code.exit(),
            3,
            "the {word} generation refusal is its own exit code"
        );
        let text = refused.envelope.render(false).unwrap();
        assert!(text.contains("dials no model"), "{text}");
        assert!(text.contains("the app"), "{text}");
        assert!(text.contains(&format!("ai {word}")), "{text}");

        let previewed = run(
            &line(&["ai", word, "generate", "--preview"]),
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
            &line(&["ai", word, "generate", "alpha"]),
            &dir,
            &mut std::io::empty(),
        );
        assert_eq!(with_operands.code.exit(), 2);
        let text = with_operands.envelope.render(false).unwrap();
        assert!(text.contains("no further words"), "{text}");
    }
}

#[test]
fn ai_document_verbs_want_one_account_and_a_known_account() {
    let dir = docs_dir();
    for word in ["digest", "report"] {
        let bare = run(&line(&["ai", word]), &dir, &mut std::io::empty());
        assert_eq!(bare.code.exit(), 2);
        let text = bare.envelope.render(false).unwrap();
        assert!(
            text.contains(&format!("ai {word} wants an account")),
            "{text}"
        );

        let extra = run(
            &line(&["ai", word, "alpha", "m1"]),
            &dir,
            &mut std::io::empty(),
        );
        assert_eq!(extra.code.exit(), 2);
        let text = extra.envelope.render(false).unwrap();
        assert!(text.contains("one account"), "{text}");

        let ghost = run(&line(&["ai", word, "ghost"]), &dir, &mut std::io::empty());
        assert_eq!(ghost.code.exit(), 1);
        let text = ghost.envelope.render(false).unwrap();
        assert!(text.contains("unknown account ghost"), "{text}");
    }
}

#[test]
fn ai_document_invocations_land_their_audit_rows() {
    let dir = docs_dir();
    for (line_words, verb, preview, outcome) in [
        (
            line(&["ai", "digest", "alpha"]),
            "ai digest",
            "account=alpha".to_owned(),
            "ok",
        ),
        (
            line(&["ai", "report", "alpha"]),
            "ai report",
            "account=alpha".to_owned(),
            "ok",
        ),
        (
            line(&["ai", "digest", "generate"]),
            "ai digest generate",
            String::new(),
            "refused",
        ),
        (
            line(&["ai", "report", "generate"]),
            "ai report generate",
            String::new(),
            "refused",
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
        assert_eq!(row, (verb.to_owned(), preview, outcome.to_owned()));
    }
}
