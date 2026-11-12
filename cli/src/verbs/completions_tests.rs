// SPDX-License-Identifier: GPL-3.0-only
//! `completions <shell>` tests: the four scripts naming every verb, the
//! usage edges, the envelope under `--json` and the raw script without it,
//! the script structure (position gating, the shell argument), and the
//! audit row the whole run layer lands. The descriptor's coverage and its
//! drift pins against the parser live in `completions_drift_tests`.

use super::completions::{self, Shell};
use super::fakes::temp_dir;
use super::{parse, run, Verb, AI_STORE_FILE, ENGINE_STORE_FILE};

/// The verb spellings the coverage and naming pins walk: every shipped
/// verb with the least arguments its parse arm accepts. Shared with the
/// drift pins in `completions_drift_tests`.
pub(super) const SPELLINGS: &[&[&str]] = &[
    &["accounts", "list"],
    &["folders", "list", "--account", "alpha"],
    &["messages", "list", "--account", "alpha"],
    &["messages", "read", "--account", "alpha", "--key", "k1"],
    &["messages", "search", "--query", "q"],
    &["messages", "mark-read", "--account", "alpha", "--key", "k1"],
    &[
        "messages",
        "set-flagged",
        "--account",
        "alpha",
        "--key",
        "k1",
    ],
    &["messages", "archive", "--account", "alpha", "--key", "k1"],
    &["messages", "trash", "--account", "alpha", "--key", "k1"],
    &[
        "drafts",
        "save",
        "--account",
        "alpha",
        "--from",
        "a@b.test",
        "--to",
        "c@d.test",
        "--subject",
        "s",
        "--plain",
        "body",
    ],
    &["drafts", "discard", "--account", "alpha", "--key", "k1"],
    &["rules", "list"],
    &[
        "rules",
        "create",
        "--name",
        "n",
        "--condition",
        "c",
        "--action",
        "mark_read",
    ],
    &["rules", "delete", "--id", "r1"],
    &["knowledge", "list", "--account", "alpha"],
    &["knowledge", "forget", "--id", "e1"],
    &["knowledge", "extract"],
    &["snippets", "list"],
    &["snippets", "create", "--title", "t", "--body", "b"],
    &["snippets", "delete", "--id", "s1"],
    &["snippets", "search"],
    &["ai", "cold", "alpha", "k1"],
    &["ai", "results", "alpha", "k1"],
    &["ai", "drafts", "alpha", "k1"],
    &["ai", "summary", "alpha", "k1"],
    &["ai", "style", "alpha"],
    &["ai", "style", "rebuild"],
    &["ai", "digest", "alpha"],
    &["ai", "report", "alpha"],
    &["ai", "digest", "generate"],
    &["ai", "report", "generate"],
    &["completions", "bash"],
];

/// Parses one spelling into its verb, the probe the coverage and drift
/// pins share.
pub(super) fn probe(words: &[&str]) -> Verb {
    let args: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    parse::parse(&args, &mut std::io::empty())
        .unwrap_or_else(|failure| panic!("{} must parse: {}", words.join(" "), failure.message))
        .verb
}

/// The verb words every script must name, derived from the vocabulary
/// itself (`Verb::name()`), never a hand-typed list.
fn verb_words() -> Vec<&'static str> {
    SPELLINGS.iter().map(|words| probe(words).name()).collect()
}

#[test]
fn every_shell_answers_a_non_empty_script_naming_every_verb() {
    for shell in completions::shells() {
        let dir = temp_dir();
        let outcome = run(
            &[
                "completions".to_owned(),
                shell.word().to_owned(),
                "--json".to_owned(),
            ],
            &dir,
            &mut std::io::empty(),
        );
        assert_eq!(outcome.code.exit(), 0, "{} must answer", shell.word());
        assert!(outcome.envelope.is_ok());
        let data = outcome.envelope.data().unwrap();
        assert_eq!(data["shell"], shell.word());
        let script = data["script"].as_str().unwrap();
        assert!(
            !script.trim().is_empty(),
            "{} script is empty",
            shell.word()
        );
        for word in verb_words() {
            assert!(
                script.contains(word),
                "{} script must name {word}: {script}",
                shell.word()
            );
        }
        // The flags ride each shell's own idiom: `--account` in bash, zsh,
        // and pwsh, `-l account` in fish.
        assert!(
            script.contains("--account") || script.contains("-l account"),
            "{} names the flags",
            shell.word()
        );
        assert!(
            script.contains("--query") || script.contains("-l query"),
            "{} names the flags",
            shell.word()
        );
    }
}

#[test]
fn each_shell_script_uses_its_shells_completion_idiom() {
    let markers = [
        (Shell::Bash, "complete -F"),
        (Shell::Zsh, "#compdef"),
        (Shell::Fish, "complete -c astra-cli"),
        (Shell::Pwsh, "Register-ArgumentCompleter"),
    ];
    for (shell, marker) in markers {
        let script = completions::script(shell);
        assert!(script.contains(marker), "{} idiom: {script}", shell.word());
    }
}

#[test]
fn the_fish_subverb_rules_stop_once_a_subverb_is_seen() {
    let script = completions::script(Shell::Fish);
    // Every subverb offering is gated on its verb being seen and none of
    // that verb's subverbs seen yet, so a later slot never re-offers the
    // subverbs as positionals.
    for line in script.lines() {
        if !line.contains("-a '") {
            continue;
        }
        assert!(
            line.contains("__fish_use_subcommand")
                || line.contains("and not __fish_seen_subcommand_from"),
            "fish subverb rule must stop once a subverb is seen: {line}"
        );
    }
    // The messages subverb rule names every messages subverb in its
    // not-seen guard, the concrete shape of the position gate.
    assert!(
        script.contains(
            "__fish_seen_subcommand_from messages; and not __fish_seen_subcommand_from \
             list read search mark-read set-flagged archive trash' -a 'list'"
        ),
        "fish messages subverb rule carries the not-seen guard: {script}"
    );
}

#[test]
fn the_completions_verb_offers_the_shell_words_in_every_shell() {
    for shell in completions::shells() {
        let script = completions::script(shell);
        for word in ["bash", "zsh", "fish", "pwsh"] {
            assert!(
                script.contains(word),
                "{} script offers the shell word {word}: {script}",
                shell.word()
            );
        }
        // The shell words ride the completions verb's own second slot, in
        // each shell's subverb idiom.
        let offering = match shell {
            Shell::Bash => {
                "completions) COMPREPLY=( $(compgen -W \"bash zsh fish pwsh\" -- \"$cur\") ) ;;"
                    .to_owned()
            }
            Shell::Zsh => "completions) subs=(bash zsh fish pwsh) ;;".to_owned(),
            Shell::Fish => "__fish_seen_subcommand_from completions; and not \
                            __fish_seen_subcommand_from bash zsh fish pwsh' -a 'bash'"
                .to_owned(),
            Shell::Pwsh => "if ($verb -eq 'completions' -and $words.Count -le 3) {\n        \
                            $candidates = @('bash', 'zsh', 'fish', 'pwsh')"
                .to_owned(),
        };
        assert!(
            script.contains(&offering),
            "{} offers the shells after completions: {script}",
            shell.word()
        );
    }
}

#[test]
fn an_unknown_shell_is_a_usage_error() {
    let dir = temp_dir();
    let outcome = run(
        &["completions".to_owned(), "tcsh".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 2);
    assert!(!outcome.envelope.is_ok());
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("shell"), "{text}");
    // The caller-typed shell word never reaches the ledger: only the CLI's
    // own vocabulary joins the verb word.
    let rows = audit_rows(&dir);
    assert_eq!(rows[0].1, "completions");
    assert_eq!(rows[0].3, "usage");
}

#[test]
fn a_missing_shell_is_a_usage_error() {
    let dir = temp_dir();
    let outcome = run(&["completions".to_owned()], &dir, &mut std::io::empty());
    assert_eq!(outcome.code.exit(), 2);
    assert!(!outcome.envelope.is_ok());
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("shell"), "{text}");
}

#[test]
fn json_answers_the_schema_one_envelope_with_shell_and_script() {
    let dir = temp_dir();
    let outcome = run(
        &[
            "completions".to_owned(),
            "bash".to_owned(),
            "--json".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(outcome.json);
    let text = outcome.envelope.render(false).unwrap();
    assert!(
        text.starts_with(r#"{"schema":1,"ok":true,"data":"#),
        "{text}"
    );
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["data"]["shell"], "bash");
    assert_eq!(value["data"]["script"], completions::script(Shell::Bash));
    assert!(outcome.raw.is_none(), "--json answers the envelope alone");
}

#[test]
fn preview_is_the_shared_non_mutating_usage_error() {
    let dir = temp_dir();
    let outcome = run(
        &[
            "completions".to_owned(),
            "bash".to_owned(),
            "--preview".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 2);
    let text = outcome.envelope.render(false).unwrap();
    assert!(
        text.contains("only the mutating verbs ship --preview"),
        "{text}"
    );
    // The shell words ride SUBVERBS, so a known shell joins the verb word
    // on a failed line, the known-subverb discipline.
    let rows = audit_rows(&dir);
    assert_eq!(rows[0].1, "completions bash");
    assert_eq!(rows[0].3, "usage");
}

#[test]
fn the_raw_form_is_the_script_not_an_envelope() {
    let dir = temp_dir();
    let outcome = run(
        &["completions".to_owned(), "fish".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let script = completions::script(Shell::Fish);
    assert_eq!(outcome.raw.as_deref(), Some(script.as_str()));
    // The envelope still carries the same payload for the audit's sake.
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["shell"], "fish");
    assert_eq!(data["script"], script);
}

#[test]
fn no_other_verb_answers_a_raw_script() {
    let dir = temp_dir();
    let outcome = run(
        &["accounts".to_owned(), "list".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(outcome.raw.is_none());
}

#[test]
fn the_audit_row_carries_the_correlation_id_and_the_shell_shape() {
    let dir = temp_dir();
    let outcome = run(
        &[
            "completions".to_owned(),
            "zsh".to_owned(),
            "--correlation-id".to_owned(),
            "turn-9".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let rows = audit_rows(&dir);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "turn-9");
    assert_eq!(rows[0].1, "completions");
    assert_eq!(rows[0].2, "shell=zsh");
    assert_eq!(rows[0].3, "ok");
}

#[test]
fn a_cold_data_dir_answers_without_the_engine() {
    let dir = temp_dir();
    assert!(!dir.exists());
    let outcome = run(
        &["completions".to_owned(), "pwsh".to_owned()],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert!(outcome.envelope.is_ok());
    assert!(dir.join(AI_STORE_FILE).exists(), "the ledger opens");
    assert!(
        !dir.join(ENGINE_STORE_FILE).exists(),
        "the engine store must not open"
    );
}

/// The audit ledger's rows as `(correlation, verb, arg_preview, outcome)`,
/// read straight from the ledger file the run wrote.
fn audit_rows(dir: &std::path::Path) -> Vec<(String, String, String, String)> {
    let conn = rusqlite::Connection::open(dir.join(AI_STORE_FILE)).unwrap();
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
