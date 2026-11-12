// SPDX-License-Identifier: GPL-3.0-only
//! The four shell templates: each renders the same vocabulary (verb
//! words, subverb words, per-verb flags) into its shell's completion
//! idiom. Simple and correct rather than feature-rich: the scripts offer
//! the vocabulary, they do not resolve values.

use std::fmt::Write as _;

use super::{Entry, Shell};

/// Every shell the generator answers, in usage-message order.
pub(super) const ALL: [Shell; 4] = [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Pwsh];

/// One verb's rendered row: its ledger spelling split into the words the
/// shells complete on, beside its flag words.
struct Row {
    /// The verb word (the first positional).
    verb: &'static str,
    /// The subverb word (the second positional), empty when the verb has
    /// none.
    sub: &'static str,
    /// The flag words the verb offers.
    flags: Vec<&'static str>,
}

/// Splits the vocabulary into rows, deriving the words from the ledger
/// spellings (`Verb::name()`). The completions verb expands into one row
/// per shell word, its second positional argument, so every template
/// offers the shells the way the other verbs offer their subverbs.
fn rows(verbs: &[(&'static str, Entry)]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (name, entry) in verbs {
        let mut words = name.split(' ');
        let verb = words.next().unwrap_or_default();
        let sub = words.next().unwrap_or_default();
        if verb == "completions" {
            for shell in ALL {
                rows.push(Row {
                    verb,
                    sub: shell.word(),
                    flags: Vec::new(),
                });
            }
        } else {
            rows.push(Row {
                verb,
                sub,
                flags: entry.flags.iter().map(|flag| flag.word()).collect(),
            });
        }
    }
    rows
}

/// The distinct verb words, in vocabulary order.
fn verb_words(rows: &[Row]) -> Vec<&'static str> {
    let mut words: Vec<&'static str> = Vec::new();
    for row in rows {
        if !words.contains(&row.verb) {
            words.push(row.verb);
        }
    }
    words
}

/// The subverb words one verb word offers, in vocabulary order, each
/// once: `ai style` and `ai style rebuild` share the `style` word.
fn sub_words(rows: &[Row], verb: &str) -> Vec<&'static str> {
    let mut words: Vec<&'static str> = Vec::new();
    for row in rows {
        if row.verb == verb && !row.sub.is_empty() && !words.contains(&row.sub) {
            words.push(row.sub);
        }
    }
    words
}

/// The vocabulary header every script carries: the full verb spellings,
/// derived from `Verb::name()`, so each script names every verb.
fn header(verbs: &[(&'static str, Entry)]) -> String {
    let names: Vec<&str> = verbs.iter().map(|(name, _)| *name).collect();
    format!("# Verbs: {}\n", names.join(", "))
}

/// The pair a row completes on: `verb sub`, or the verb word alone when
/// there is no subverb.
fn pair(row: &Row) -> String {
    if row.sub.is_empty() {
        row.verb.to_owned()
    } else {
        format!("{} {}", row.verb, row.sub)
    }
}

/// The bash script: a `complete -F` function completing the verb word,
/// then the subverb, then the pair's flags.
pub(super) fn bash(verbs: &[(&'static str, Entry)]) -> String {
    let rows = rows(verbs);
    let mut s = String::new();
    s.push_str("# astra-cli bash completion\n");
    s.push_str(&header(verbs));
    s.push_str("_astra_cli() {\n");
    s.push_str("    local cur verb sub\n");
    s.push_str("    cur=\"${COMP_WORDS[COMP_CWORD]}\"\n");
    s.push_str("    verb=\"${COMP_WORDS[1]}\"\n");
    s.push_str("    sub=\"${COMP_WORDS[2]}\"\n");
    s.push_str("    if [[ ${COMP_CWORD} -eq 1 ]]; then\n");
    writeln!(
        s,
        "        COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
        verb_words(&rows).join(" ")
    )
    .unwrap();
    s.push_str("        return 0\n");
    s.push_str("    fi\n");
    s.push_str("    if [[ ${COMP_CWORD} -eq 2 ]]; then\n");
    s.push_str("        case \"$verb\" in\n");
    for verb in verb_words(&rows) {
        let subs = sub_words(&rows, verb);
        if subs.is_empty() {
            continue;
        }
        writeln!(
            s,
            "            {verb}) COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;",
            subs.join(" ")
        )
        .unwrap();
    }
    s.push_str("        esac\n");
    s.push_str("        return 0\n");
    s.push_str("    fi\n");
    s.push_str("    case \"$verb $sub\" in\n");
    for row in &rows {
        if row.flags.is_empty() {
            continue;
        }
        writeln!(
            s,
            "        \"{}\") COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;",
            pair(row),
            row.flags.join(" ")
        )
        .unwrap();
    }
    s.push_str("    esac\n");
    s.push_str("    return 0\n");
    s.push_str("}\n");
    s.push_str("complete -F _astra_cli astra-cli\n");
    s
}

/// The zsh script: a `#compdef` function offering the verb words, the
/// subverbs, and the pair's flags through `_describe`.
pub(super) fn zsh(verbs: &[(&'static str, Entry)]) -> String {
    let rows = rows(verbs);
    let mut s = String::new();
    s.push_str("#compdef astra-cli\n");
    s.push_str(&header(verbs));
    s.push_str("_astra_cli() {\n");
    s.push_str("    local -a verbs\n");
    s.push_str("    verbs=(\n");
    for verb in verb_words(&rows) {
        writeln!(s, "        '{verb}'").unwrap();
    }
    s.push_str("    )\n");
    s.push_str("    if (( CURRENT == 2 )); then\n");
    s.push_str("        _describe 'verb' verbs\n");
    s.push_str("        return\n");
    s.push_str("    fi\n");
    s.push_str("    local verb=$words[2]\n");
    s.push_str("    local sub=$words[3]\n");
    s.push_str("    if (( CURRENT == 3 )); then\n");
    s.push_str("        local -a subs\n");
    s.push_str("        case $verb in\n");
    for verb in verb_words(&rows) {
        let subs = sub_words(&rows, verb);
        if subs.is_empty() {
            continue;
        }
        writeln!(s, "            {verb}) subs=({}) ;;", subs.join(" ")).unwrap();
    }
    s.push_str("        esac\n");
    s.push_str("        _describe 'subverb' subs\n");
    s.push_str("        return\n");
    s.push_str("    fi\n");
    s.push_str("    local -a flags\n");
    s.push_str("    case \"$verb $sub\" in\n");
    for row in &rows {
        if row.flags.is_empty() {
            continue;
        }
        writeln!(
            s,
            "        \"{}\") flags=({}) ;;",
            pair(row),
            row.flags.join(" ")
        )
        .unwrap();
    }
    s.push_str("    esac\n");
    s.push_str("    _describe 'flag' flags\n");
    s.push_str("}\n");
    s.push_str("_astra_cli \"$@\"\n");
    s
}

/// The fish script: `complete -c` rules, one per verb word, subverb, and
/// flag. The verb words fire only at the subcommand position; a subverb
/// fires only while its verb is seen and none of that verb's subverbs is
/// seen yet, so a later slot never re-offers the subverbs; a flag fires
/// once its verb (and subverb, when it has one) is seen.
pub(super) fn fish(verbs: &[(&'static str, Entry)]) -> String {
    let rows = rows(verbs);
    let mut s = String::new();
    s.push_str("# astra-cli fish completion\n");
    s.push_str(&header(verbs));
    s.push_str("complete -c astra-cli -f\n");
    for verb in verb_words(&rows) {
        writeln!(
            s,
            "complete -c astra-cli -n '__fish_use_subcommand' -a '{verb}'"
        )
        .unwrap();
    }
    for row in &rows {
        if !row.sub.is_empty() {
            let subs = sub_words(&rows, row.verb);
            writeln!(
                s,
                "complete -c astra-cli -n '__fish_seen_subcommand_from {}; and not __fish_seen_subcommand_from {}' -a '{}'",
                row.verb,
                subs.join(" "),
                row.sub
            )
            .unwrap();
        }
        for flag in &row.flags {
            let condition = if row.sub.is_empty() {
                format!("__fish_seen_subcommand_from {}", row.verb)
            } else {
                format!(
                    "__fish_seen_subcommand_from {}; and __fish_seen_subcommand_from {}",
                    row.verb, row.sub
                )
            };
            writeln!(
                s,
                "complete -c astra-cli -n '{condition}' -l {}",
                flag.trim_start_matches('-')
            )
            .unwrap();
        }
    }
    s
}

/// The pwsh script: a `Register-ArgumentCompleter` block offering the verb
/// words, the subverbs, and the pair's flags as completion results.
pub(super) fn pwsh(verbs: &[(&'static str, Entry)]) -> String {
    let rows = rows(verbs);
    let mut s = String::new();
    s.push_str("# astra-cli pwsh completion\n");
    s.push_str(&header(verbs));
    s.push_str("Register-ArgumentCompleter -CommandName 'astra-cli' -ScriptBlock {\n");
    s.push_str("    param($wordToComplete, $commandAst, $cursorPosition)\n");
    s.push_str("    $words = $commandAst.ToString() -split '\\s+'\n");
    s.push_str("    $verb = if ($words.Count -gt 1) { $words[1] } else { '' }\n");
    s.push_str("    $sub = if ($words.Count -gt 2) { $words[2] } else { '' }\n");
    s.push_str("    $candidates = @()\n");
    s.push_str("    if ($words.Count -le 2) {\n");
    writeln!(
        s,
        "        $candidates = @({})",
        quoted_list(&verb_words(&rows))
    )
    .unwrap();
    s.push_str("    }\n");
    for verb in verb_words(&rows) {
        let subs = sub_words(&rows, verb);
        if subs.is_empty() {
            continue;
        }
        writeln!(s, "    if ($verb -eq '{verb}' -and $words.Count -le 3) {{").unwrap();
        writeln!(s, "        $candidates = @({})", quoted_list(&subs)).unwrap();
        s.push_str("    }\n");
    }
    for row in &rows {
        if row.flags.is_empty() {
            continue;
        }
        let condition = if row.sub.is_empty() {
            format!("$verb -eq '{}'", row.verb)
        } else {
            format!("$verb -eq '{}' -and $sub -eq '{}'", row.verb, row.sub)
        };
        writeln!(s, "    if ({condition}) {{").unwrap();
        writeln!(s, "        $candidates = @({})", quoted_list(&row.flags)).unwrap();
        s.push_str("    }\n");
    }
    s.push_str(
        "    $candidates | Where-Object { $_ -like \"$wordToComplete*\" } | ForEach-Object {\n",
    );
    s.push_str("        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    s
}

/// Renders words as a pwsh array body: `'a', 'b', 'c'`.
fn quoted_list(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| format!("'{word}'"))
        .collect::<Vec<_>>()
        .join(", ")
}
