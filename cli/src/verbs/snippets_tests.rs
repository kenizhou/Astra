// SPDX-License-Identifier: GPL-3.0-only
//! `snippets list`, `snippets create`, and `snippets delete` tests: the
//! library's round trip through the snippet store (migration 0008) with
//! the whole bodies the composer's insert consumes, the create's
//! one-source body discipline and its preview's equality with what a run
//! stores, the delete with its preview and its loud unknown-id error, the
//! refused `snippets search` spelling under the never-dials rule, and the
//! audit rows every invocation lands.

use astra_ai::{AiStore, NewSnippet};

use super::fakes::temp_dir;
use super::run;

/// One command line as the `run` layer takes it.
fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// Creates one snippet and hands its minted id back.
fn create(dir: &std::path::Path, title: &str, body: &str) -> String {
    let outcome = run(
        &line(&["snippets", "create", "--title", title, "--body", body]),
        dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0, "the seed create must succeed");
    outcome.envelope.data().unwrap()["snippet"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn snippets_create_stores_the_snippet_and_lists_it_back() {
    let dir = temp_dir();
    let outcome = run(
        &line(&[
            "snippets",
            "create",
            "--title",
            "Address change",
            "--body",
            "We moved: the new address is 42 Harbour Road.",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["state"], "created");
    let id = data["snippet"]["id"].as_str().unwrap();
    assert!(!id.is_empty(), "the created snippet carries its minted id");
    assert_eq!(data["snippet"]["title"], "Address change");
    assert_eq!(
        data["snippet"]["body"],
        "We moved: the new address is 42 Harbour Road."
    );
    assert!(data["snippet"]["created_at"].as_i64().unwrap() > 0);

    create(&dir, "Invoice chase", "Attached is the invoice again.");
    let listed = run(&line(&["snippets", "list"]), &dir, &mut std::io::empty());
    assert_eq!(listed.code.exit(), 0);
    let snippets = listed.envelope.data().unwrap()["snippets"].clone();
    let mut titles: Vec<String> = snippets
        .as_array()
        .unwrap()
        .iter()
        .map(|snippet| snippet["title"].as_str().unwrap().to_owned())
        .collect();
    titles.sort();
    assert_eq!(
        titles,
        vec!["Address change".to_owned(), "Invoice chase".to_owned()],
        "both stored snippets list back"
    );
    let by_title = |title: &str| {
        snippets
            .as_array()
            .unwrap()
            .iter()
            .find(|snippet| snippet["title"] == title)
            .unwrap()
            .clone()
    };
    assert_eq!(
        by_title("Address change")["id"],
        id,
        "the listing carries the identity the create answered"
    );
    assert!(
        by_title("Invoice chase")["body"]
            .as_str()
            .unwrap()
            .contains("invoice"),
        "the whole body rides the read, the whole-body discipline"
    );
    assert!(
        by_title("Invoice chase")["updated_at"].as_i64().unwrap() > 0
            && by_title("Invoice chase")["created_at"].as_i64().unwrap() > 0,
        "the store's stamps ride: {snippets}"
    );
}

#[test]
fn the_create_preview_prints_the_exact_snippet_a_run_stores() {
    let dir = temp_dir();
    let outcome = run(
        &line(&[
            "snippets",
            "create",
            "--title",
            "Thanks",
            "--body",
            "Thanks for the quick turnaround.",
            "--preview",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let preview = outcome.envelope.data().unwrap().clone();
    assert_eq!(preview["action"], "create_snippet");
    let shown = &preview["snippet"];
    assert_eq!(shown["title"], "Thanks");
    assert_eq!(shown["body"], "Thanks for the quick turnaround.");
    assert!(
        run(&line(&["snippets", "list"]), &dir, &mut std::io::empty())
            .envelope
            .data()
            .unwrap()["snippets"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a preview stores nothing"
    );

    // Equality against what a run actually stores, read back through the
    // list. Each invocation mints its own id, so the identity is the
    // run's, not the preview's.
    run(
        &line(&[
            "snippets",
            "create",
            "--title",
            "Thanks",
            "--body",
            "Thanks for the quick turnaround.",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    let stored = run(&line(&["snippets", "list"]), &dir, &mut std::io::empty())
        .envelope
        .data()
        .unwrap()["snippets"][0]
        .clone();
    assert!(!stored["id"].as_str().unwrap().is_empty());
    assert_eq!(stored["title"], shown["title"]);
    assert_eq!(stored["body"], shown["body"]);
}

#[test]
fn snippets_create_takes_one_body_source_with_content() {
    let dir = temp_dir();
    let body_file = dir.join("body.txt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&body_file, "Filed from the disk.").unwrap();
    for words in [
        line(&["snippets", "create", "--title", "T"]),
        line(&["snippets", "create", "--body", "B"]),
        line(&[
            "snippets",
            "create",
            "--title",
            "T",
            "--body",
            "B",
            "--body-file",
            "f",
        ]),
        line(&["snippets", "create", "--title", "  ", "--body", "B"]),
        line(&["snippets", "create", "--title", "T", "--body", "   "]),
        line(&[
            "snippets",
            "create",
            "--title",
            "T",
            "--body-file",
            "nowhere.txt",
        ]),
    ] {
        let outcome = run(&words, &dir, &mut std::io::empty());
        assert_eq!(outcome.code.exit(), 2, "{words:?} must be usage");
    }
    // The well-formed shapes beside them parse: the arg body and the file
    // body each store one snippet.
    assert_eq!(
        run(
            &line(&["snippets", "create", "--title", "T", "--body", "B"]),
            &dir,
            &mut std::io::empty()
        )
        .code
        .exit(),
        0
    );
    let filed = run(
        &line(&[
            "snippets",
            "create",
            "--title",
            "Filed",
            "--body-file",
            body_file.to_str().unwrap(),
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(filed.code.exit(), 0);
    assert_eq!(
        filed.envelope.data().unwrap()["snippet"]["body"],
        "Filed from the disk.",
        "the file body crossed"
    );
}

#[test]
fn snippets_delete_removes_the_snippet_and_names_an_unknown_id() {
    let dir = temp_dir();
    let id = create(&dir, "Old signoff", "Cheers, Grace");
    let outcome = run(
        &line(&["snippets", "delete", "--id", &id]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["id"], id);
    assert_eq!(data["state"], "deleted");
    assert!(
        run(&line(&["snippets", "list"]), &dir, &mut std::io::empty())
            .envelope
            .data()
            .unwrap()["snippets"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the snippet is gone"
    );

    let again = run(
        &line(&["snippets", "delete", "--id", &id]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(again.code.exit(), 1);
    let text = again.envelope.render(false).unwrap();
    assert!(text.contains(&id), "the failure names the id: {text}");

    let previewed = run(
        &line(&["snippets", "delete", "--id", "any", "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(previewed.code.exit(), 0);
    let preview = previewed.envelope.data().unwrap().clone();
    assert_eq!(preview["action"], "delete_snippet");
    assert_eq!(preview["id"], "any");
}

#[test]
fn snippets_search_refuses_under_the_never_dials_rule() {
    let dir = temp_dir();
    create(&dir, "Thanks", "Thanks for the note.");
    let outcome = run(
        &line(&["snippets", "search", "--query", "gratitude"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 3, "the refusal is its own exit code");
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("dials no model"), "{text}");
    assert!(text.contains("the app"), "{text}");
    assert!(text.contains("snippets list"), "{text}");
    let previewed = run(
        &line(&["snippets", "search", "--preview"]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(previewed.code.exit(), 2);
}

#[test]
fn every_snippets_invocation_lands_its_audit_row() {
    let dir = temp_dir();
    let body_file = dir.join("body.txt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&body_file, "Filed body.").unwrap();
    let lines = [
        (
            line(&["snippets", "list"]),
            "snippets list",
            String::new(),
            "ok".to_owned(),
        ),
        (
            line(&[
                "snippets",
                "create",
                "--title",
                "Thanks",
                "--body",
                "Thanks for the quick turnaround.",
            ]),
            "snippets create",
            "title_chars=6 body_chars=32 source=arg".to_owned(),
            "ok".to_owned(),
        ),
        (
            line(&[
                "snippets",
                "create",
                "--title",
                "Filed",
                "--body-file",
                body_file.to_str().unwrap(),
            ]),
            "snippets create",
            "title_chars=5 body_chars=11 source=file".to_owned(),
            "ok".to_owned(),
        ),
        (
            line(&["snippets", "delete", "--id", "ghost"]),
            "snippets delete",
            "id=ghost".to_owned(),
            "error".to_owned(),
        ),
        (
            line(&["snippets", "search"]),
            "snippets search",
            String::new(),
            "refused".to_owned(),
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

#[test]
fn the_snippet_store_is_shared_with_the_hosts_listing() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // A row the app-side surface would have written, landed through the
    // store's own API: the CLI reads the one library every surface shares.
    let store = AiStore::open(&dir.join(super::AI_STORE_FILE)).unwrap();
    store
        .insert_snippet(&NewSnippet {
            id: astra_ai::SnippetId::new("s-app".to_owned()),
            title: "Host written".to_owned(),
            body: "Written by the app surface.".to_owned(),
        })
        .unwrap();
    drop(store);
    let listed = run(&line(&["snippets", "list"]), &dir, &mut std::io::empty());
    assert_eq!(listed.code.exit(), 0);
    let snippets = listed.envelope.data().unwrap()["snippets"].clone();
    assert_eq!(snippets.as_array().unwrap().len(), 1);
    assert_eq!(snippets[0]["id"], "s-app");
    assert_eq!(snippets[0]["title"], "Host written");
}
