// SPDX-License-Identifier: GPL-3.0-only
//! `accounts list` tests: the index read, the empty-data-dir answer, and the
//! envelope and audit the whole run layer gives.

use super::fakes::{temp_dir, write_index};
use super::run;

#[test]
fn accounts_list_answers_the_index_in_order() {
    let dir = temp_dir();
    write_index(&dir, &["zeta", "alpha"]);
    let outcome = run(
        &[
            "accounts".to_owned(),
            "list".to_owned(),
            "--json".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(outcome.envelope.is_ok());
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["accounts"], serde_json::json!(["zeta", "alpha"]));
}

#[test]
fn accounts_list_answers_empty_when_no_index_exists() {
    let dir = temp_dir();
    let outcome = run(
        &["accounts".to_owned(), "list".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert_eq!(
        outcome.envelope.data().unwrap()["accounts"],
        serde_json::json!([])
    );
}

#[test]
fn every_invocation_lands_one_audit_row_with_the_correlation_id() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let outcome = run(
        &[
            "accounts".to_owned(),
            "list".to_owned(),
            "--correlation-id".to_owned(),
            "turn-7".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let rows = audit_rows(&dir);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "turn-7");
    assert_eq!(rows[0].1, "accounts list");
    assert_eq!(rows[0].2, "");
    assert_eq!(rows[0].3, "ok");
}

#[test]
fn a_generated_correlation_id_lands_when_none_was_passed() {
    let dir = temp_dir();
    run(
        &["accounts".to_owned(), "list".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    let rows = audit_rows(&dir);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].0.is_empty());
}

#[test]
fn an_unparsable_line_audits_as_usage_and_exits_two() {
    let dir = temp_dir();
    let outcome = run(
        &["frobnicate".to_owned(), "list".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 2);
    let rows = audit_rows(&dir);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].3, "usage");
}

#[test]
fn a_known_subverb_joins_the_ledgers_verb_word() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    // `messages read` is the CLI's own vocabulary; the missing --key makes
    // it a usage failure, and the verb word keeps its subverb.
    run(
        &["messages".to_owned(), "read".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(audit_rows(&dir)[0].1, "messages read");
}

#[test]
fn a_caller_typed_positional_never_reaches_the_ledger() {
    let dir = temp_dir();
    run(
        &["delete-me".to_owned(), "now".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(audit_rows(&dir)[0].1, "delete-me");
}

#[test]
fn send_class_verbs_are_parsed_then_refused_with_exit_three() {
    let dir = temp_dir();
    for word in ["send", "queue"] {
        let outcome = run(
            &[word.to_owned(), "--to".to_owned(), "a@b.test".to_owned()],
            &dir,
            &mut std::io::empty(),
        );
        assert_eq!(outcome.code.exit(), 3, "{word} must exit 3");
        assert!(!outcome.envelope.is_ok());
        let text = outcome.envelope.render(false).unwrap();
        assert!(
            text.contains("sending is a human action"),
            "{word} refusal names the human-send rule: {text}"
        );
        let rows = audit_rows(&dir);
        assert_eq!(rows.last().unwrap().1, word);
        assert_eq!(rows.last().unwrap().3, "refused");
    }
}

#[test]
fn a_missing_flag_is_a_usage_error_naming_the_flag() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let outcome = run(
        &["folders".to_owned(), "list".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 2);
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("--account"), "{text}");
}

#[test]
fn an_unknown_account_is_a_runtime_error_naming_it() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let outcome = run(
        &[
            "folders".to_owned(),
            "list".to_owned(),
            "--account".to_owned(),
            "ghost".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 1);
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("unknown account ghost"), "{text}");
}

/// The audit ledger's rows as `(correlation, verb, arg_preview, outcome)`,
/// read straight from the ledger file the run wrote.
fn audit_rows(dir: &std::path::Path) -> Vec<(String, String, String, String)> {
    let conn = rusqlite::Connection::open(dir.join(super::AI_STORE_FILE)).unwrap();
    let mut statement = conn
        .prepare("SELECT correlation_id, verb, arg_preview, outcome FROM audit_log ORDER BY rowid")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
