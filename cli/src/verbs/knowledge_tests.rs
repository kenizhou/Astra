// SPDX-License-Identifier: GPL-3.0-only
//! `knowledge list` and `knowledge forget` tests: the listing read over the
//! knowledge store the app's extraction fills (migration 0007), newest
//! first with every source half carried; the forget round trip with its
//! preview and its loud unknown-id error; the refused `knowledge extract`
//! spelling under the never-dials rule; and the audit rows every
//! invocation lands.

use astra_ai::{AiStore, CorrelationId, NewKnowledgeEntry};

use super::fakes::{temp_dir, write_index};
use super::run;

/// One command line as the `run` layer takes it.
fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// One seeded entry, `id`, `kind`, and `text` the caller's, sourced from
/// the pair every seeded row shares.
fn entry(id: &str, kind: &str, text: &str) -> NewKnowledgeEntry {
    NewKnowledgeEntry {
        id: astra_ai::KnowledgeId::new(id.to_owned()),
        account: "alpha".to_owned(),
        kind: kind.to_owned(),
        text: text.to_owned(),
        source_message_account: "alpha".to_owned(),
        source_message_key: "m9".to_owned(),
        correlation: CorrelationId::new("turn-old".to_owned()),
    }
}

/// A data dir whose index names `alpha` and whose knowledge store holds
/// the three seeded entries.
fn seeded() -> std::path::PathBuf {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    let store = AiStore::open(&dir.join(super::AI_STORE_FILE)).unwrap();
    store
        .insert_knowledge(&entry("k1", "fact", "The launch lands on Friday."))
        .unwrap();
    store
        .insert_knowledge(&entry("k2", "preference", "The user prefers plain text."))
        .unwrap();
    store
        .insert_knowledge(&entry("k3", "contact", "Ada reviews the user's work."))
        .unwrap();
    dir
}

#[test]
fn knowledge_list_answers_the_accounts_entries_newest_first() {
    let dir = seeded();
    let outcome = run(
        &line(&["knowledge", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let entries = outcome.envelope.data().unwrap()["entries"].clone();
    assert_eq!(
        entries
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["k3", "k2", "k1"],
        "the store's newest-first order crosses whole",
    );
    let first = &entries[0];
    assert_eq!(first["kind"], "contact");
    assert_eq!(first["text"], "Ada reviews the user's work.");
    assert_eq!(first["source_account"], "alpha");
    assert_eq!(first["source_key"], "m9");
    assert!(
        first["created_at"].as_i64().unwrap() > 0,
        "the store's stamp rides: {first}"
    );

    let bare = temp_dir();
    write_index(&bare, &["alpha"]);
    let empty = run(
        &line(&["knowledge", "list", "--account", "alpha"]),
        &bare,
        &mut std::io::empty(),
    );
    assert_eq!(empty.code.exit(), 0);
    assert!(
        empty.envelope.data().unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty(),
        "an account no extraction touched answers empty"
    );
}

#[test]
fn knowledge_list_wants_its_account_flag_and_a_known_account() {
    let dir = seeded();
    let missing = run(&line(&["knowledge", "list"]), &dir, &mut std::io::empty());
    assert_eq!(missing.code.exit(), 2);
    let text = missing.envelope.render(false).unwrap();
    assert!(text.contains("--account"), "{text}");

    let ghost = run(
        &line(&["knowledge", "list", "--account", "ghost"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(ghost.code.exit(), 1);
    let text = ghost.envelope.render(false).unwrap();
    assert!(text.contains("unknown account ghost"), "{text}");
}

#[test]
fn knowledge_forget_removes_the_entry_and_names_an_unknown_id() {
    let dir = seeded();
    let outcome = run(
        &line(&["knowledge", "forget", "--id", "k2"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["id"], "k2");
    assert_eq!(data["state"], "forgotten");
    let listed = run(
        &line(&["knowledge", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    let ids: Vec<&str> = listed.envelope.data().unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["k3", "k1"], "the forgotten row went alone");

    let again = run(
        &line(&["knowledge", "forget", "--id", "k2"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(
        again.code.exit(),
        1,
        "an id no entry carries is a runtime error"
    );
    let text = again.envelope.render(false).unwrap();
    assert!(text.contains("k2"), "the failure names the id: {text}");
}

#[test]
fn the_forget_preview_names_the_exact_forgetting_and_writes_nothing() {
    let dir = seeded();
    let outcome = run(
        &line(&["knowledge", "forget", "--id", "k1", "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let preview = outcome.envelope.data().unwrap().clone();
    assert_eq!(preview["action"], "forget_knowledge");
    assert_eq!(preview["id"], "k1");
    assert_eq!(
        run(
            &line(&["knowledge", "list", "--account", "alpha"]),
            &dir,
            &mut std::io::empty(),
        )
        .envelope
        .data()
        .unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "the preview forgot nothing"
    );
}

#[test]
fn knowledge_extract_refuses_under_the_never_dials_rule() {
    let dir = seeded();
    let outcome = run(
        &line(&["knowledge", "extract", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 3, "the refusal is its own exit code");
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("dials no model"), "{text}");
    assert!(text.contains("the app"), "{text}");
    assert!(text.contains("knowledge list"), "{text}");
    // A preview has no action to describe: the shared non-mutating usage
    // error, the refused style rebuild's discipline.
    let previewed = run(
        &line(&["knowledge", "extract", "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(previewed.code.exit(), 2);
}

#[test]
fn every_knowledge_invocation_lands_its_audit_row() {
    let dir = seeded();
    let lines = [
        (
            line(&["knowledge", "list", "--account", "alpha"]),
            "knowledge list",
            "account=alpha".to_owned(),
            "ok".to_owned(),
        ),
        (
            line(&["knowledge", "forget", "--id", "k3", "--preview"]),
            "knowledge forget",
            "id=k3".to_owned(),
            "preview".to_owned(),
        ),
        (
            line(&["knowledge", "extract"]),
            "knowledge extract",
            String::new(),
            "refused".to_owned(),
        ),
        (
            line(&["knowledge", "forget"]),
            "knowledge forget",
            String::new(),
            "usage".to_owned(),
        ),
    ];
    for (at, (words, verb, preview, outcome)) in lines.into_iter().enumerate() {
        let correlation = format!("turn-{at}-{}", verb.replace(' ', "-"));
        let mut words = words;
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
        assert_eq!(row, (verb.to_owned(), preview, outcome));
    }
}
