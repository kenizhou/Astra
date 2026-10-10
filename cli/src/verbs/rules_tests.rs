// SPDX-License-Identifier: GPL-3.0-only
//! `rules list`, `rules create`, and `rules delete` tests: the round trip
//! through the rules store the app's incoming pass shares, the preview's
//! equality with what a run actually stores, the closed action set at
//! parse, the loud rejection of a stored payload outside the set, and the
//! audit rows every invocation lands.

use super::fakes::temp_dir;
use super::run;

/// One command line as the `run` layer takes it.
fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// Creates one rule and hands its minted id back.
fn create(dir: &std::path::Path, action: &str) -> String {
    let outcome = run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Marketing",
            "--condition",
            "The message is from the marketing team.",
            "--action",
            action,
        ]),
        dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0, "the seed create must succeed");
    outcome.envelope.data().unwrap()["rule"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn rules_create_stores_the_rule_and_lists_it_back() {
    let dir = temp_dir();
    let outcome = run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Marketing",
            "--condition",
            "The message is from the marketing team.",
            "--action",
            "mark_read",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["state"], "created");
    let id = data["rule"]["id"].as_str().unwrap();
    assert!(!id.is_empty(), "the created rule carries its minted id");

    let listed = run(&line(&["rules", "list"]), &dir, &mut std::io::empty());
    assert_eq!(listed.code.exit(), 0);
    let rules = listed.envelope.data().unwrap()["rules"].clone();
    assert_eq!(rules.as_array().unwrap().len(), 1);
    assert_eq!(rules[0]["id"], id);
    assert_eq!(rules[0]["name"], "Marketing");
    assert_eq!(
        rules[0]["condition"],
        "The message is from the marketing team."
    );
    assert_eq!(rules[0]["actions"], serde_json::json!(["mark_read"]));
    assert_eq!(rules[0]["enabled"], true);
    assert!(
        rules[0]["created_at"].as_i64().unwrap() > 0,
        "the store's stamp rides: {rules}"
    );
}

#[test]
fn rules_create_repeats_actions_and_rejects_words_outside_the_set() {
    let dir = temp_dir();
    let outcome = run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Receipts",
            "--condition",
            "The message is a receipt.",
            "--action",
            "mark_read",
            "--action",
            "set_flagged",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(
        data["rule"]["actions"],
        serde_json::json!(["mark_read", "set_flagged"]),
        "a repeated --action collects"
    );

    let refused = run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Bad",
            "--condition",
            "Whatever.",
            "--action",
            "archive",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(refused.code.exit(), 2, "an unknown action word is usage");
    let text = refused.envelope.render(false).unwrap();
    assert!(
        text.contains("archive") && text.contains("mark_read"),
        "the refusal names the word and the closed set: {text}"
    );
    assert!(
        run(&line(&["rules", "list"]), &dir, &mut std::io::empty())
            .envelope
            .data()
            .unwrap()["rules"]
            .as_array()
            .unwrap()
            .len()
            == 1,
        "the refused line stored nothing"
    );
}

#[test]
fn rules_create_wants_a_name_a_condition_and_an_action() {
    let dir = temp_dir();
    for words in [
        line(&[
            "rules",
            "create",
            "--condition",
            "C",
            "--action",
            "mark_read",
        ]),
        line(&["rules", "create", "--name", "N", "--action", "mark_read"]),
        line(&["rules", "create", "--name", "N", "--condition", "C"]),
        line(&[
            "rules",
            "create",
            "--name",
            "  ",
            "--condition",
            "C",
            "--action",
            "mark_read",
        ]),
        line(&[
            "rules",
            "create",
            "--name",
            "N",
            "--condition",
            "",
            "--action",
            "mark_read",
        ]),
        line(&[
            "rules",
            "create",
            "--name",
            "N",
            "--condition",
            "C",
            "--action",
            "mark_read",
            "--action",
            "mark_read",
        ]),
    ] {
        let outcome = run(&words, &dir, &mut std::io::empty());
        assert_eq!(outcome.code.exit(), 2, "{words:?} must be usage");
    }
    // The well-formed shape beside them parses, proving the failures are
    // the checks and not the grammar.
    let outcome = run(
        &line(&[
            "rules",
            "create",
            "--name",
            "N",
            "--condition",
            "C",
            "--action",
            "mark_read",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
}

#[test]
fn the_create_preview_prints_the_exact_rule_a_run_stores() {
    let dir = temp_dir();
    let outcome = run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Newsletters",
            "--condition",
            "The message is a recurring bulk mailing.",
            "--action",
            "mark_read",
            "--action",
            "set_flagged",
            "--preview",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let preview = outcome.envelope.data().unwrap().clone();
    assert_eq!(preview["action"], "create_rule");
    let shown = &preview["rule"];
    assert_eq!(shown["name"], "Newsletters");
    assert_eq!(
        shown["condition"],
        "The message is a recurring bulk mailing."
    );
    assert_eq!(
        shown["actions"],
        serde_json::json!(["mark_read", "set_flagged"])
    );
    assert_eq!(shown["enabled"], true);
    assert!(
        run(&line(&["rules", "list"]), &dir, &mut std::io::empty())
            .envelope
            .data()
            .unwrap()["rules"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a preview stores nothing"
    );

    // Equality against what a run actually stores: the same fields, one
    // for one, read back through the list. Each invocation mints its own
    // id, so the identity is the run's, not the preview's.
    run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Newsletters",
            "--condition",
            "The message is a recurring bulk mailing.",
            "--action",
            "mark_read",
            "--action",
            "set_flagged",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    let stored = run(&line(&["rules", "list"]), &dir, &mut std::io::empty())
        .envelope
        .data()
        .unwrap()["rules"][0]
        .clone();
    assert!(!stored["id"].as_str().unwrap().is_empty());
    assert_eq!(stored["name"], shown["name"]);
    assert_eq!(stored["condition"], shown["condition"]);
    assert_eq!(stored["enabled"], shown["enabled"]);
    assert_eq!(stored["actions"], shown["actions"]);
}

#[test]
fn rules_delete_removes_the_rule_and_names_an_unknown_id() {
    let dir = temp_dir();
    let id = create(&dir, "mark_read");
    let outcome = run(
        &line(&["rules", "delete", "--id", &id]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["id"], id);
    assert_eq!(data["state"], "deleted");
    assert!(
        run(&line(&["rules", "list"]), &dir, &mut std::io::empty())
            .envelope
            .data()
            .unwrap()["rules"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the rule is gone"
    );

    let again = run(
        &line(&["rules", "delete", "--id", &id]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(again.code.exit(), 1, "an id no rule carries is runtime");
    let text = again.envelope.render(false).unwrap();
    assert!(text.contains(&id), "the failure names the id: {text}");
}

#[test]
fn the_delete_preview_names_the_exact_deletion_and_writes_nothing() {
    let dir = temp_dir();
    let id = create(&dir, "mark_read");
    let outcome = run(
        &line(&["rules", "delete", "--id", &id, "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let preview = outcome.envelope.data().unwrap().clone();
    assert_eq!(preview["action"], "delete_rule");
    assert_eq!(preview["id"], id);
    assert_eq!(
        run(&line(&["rules", "list"]), &dir, &mut std::io::empty())
            .envelope
            .data()
            .unwrap()["rules"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the preview deleted nothing"
    );
}

#[test]
fn rules_list_fails_loudly_on_a_stored_payload_outside_the_set() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // The broken writer the loud rejection guards against: a rule whose
    // actions payload names a word the closed set does not carry.
    let store = astra_ai::AiStore::open(&dir.join(super::AI_STORE_FILE)).unwrap();
    store
        .create_rule(&astra_ai::NewRule {
            id: astra_ai::RuleId::new("broken-rule".to_owned()),
            name: "Broken".to_owned(),
            condition: "Whatever.".to_owned(),
            actions: r#"["shred"]"#.to_owned(),
            enabled: true,
        })
        .unwrap();
    drop(store);

    let outcome = run(&line(&["rules", "list"]), &dir, &mut std::io::empty());
    assert_eq!(outcome.code.exit(), 1);
    let text = outcome.envelope.render(false).unwrap();
    assert!(
        text.contains("broken-rule") && text.contains("shred"),
        "the failure names the rule and the word: {text}"
    );
}

#[test]
fn every_rules_invocation_lands_its_audit_row() {
    let dir = temp_dir();
    // A successful create: the verb word keeps its subverb, the preview
    // carries shapes only, and the outcome says ok.
    run(
        &line(&[
            "rules",
            "create",
            "--name",
            "Marketing",
            "--condition",
            "The message is from the marketing team.",
            "--action",
            "mark_read",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    let rows = audit_rows(&dir);
    assert_eq!(rows[0].1, "rules create");
    assert_eq!(rows[0].2, "name_chars=9 condition_chars=39 actions=1");
    assert_eq!(rows[0].3, "ok");

    // A preview answers preview and stores nothing.
    let id = create(&dir, "mark_read");
    run(
        &line(&["rules", "delete", "--id", &id, "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(audit_rows(&dir).last().unwrap().3, "preview");

    // A usage failure still lands its row with the two-word verb.
    run(
        &line(&["rules", "create", "--name", "N"]),
        &dir,
        &mut std::io::empty(),
    );
    let last = audit_rows(&dir).last().unwrap().clone();
    assert_eq!(
        (last.1.as_str(), last.3.as_str()),
        ("rules create", "usage")
    );
}

/// The audit ledger's rows as `(correlation, verb, arg_preview, outcome)`,
/// read straight from the ledger file the runs wrote.
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
