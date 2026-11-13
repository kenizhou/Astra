// SPDX-License-Identifier: GPL-3.0-only
//! The completion descriptor's pins: coverage of every verb, and the
//! drift pins that hold the descriptor's flag vocabulary against the
//! parser for the representative verbs. Split from `completions_tests`
//! so each file stays under the gate's line cap.

use super::completions;
use super::completions_tests::{probe, SPELLINGS};
use super::{parse, Verb};

#[test]
fn the_descriptor_covers_every_verb() {
    for words in SPELLINGS {
        let verb = probe(words);
        // The exhaustive match answers for every verb; each entry's flags
        // are a set, no word twice.
        let entry = completions::descriptor(&verb);
        let mut seen: Vec<&str> = Vec::new();
        for flag in entry.flags {
            assert!(
                !seen.contains(&flag.word()),
                "{} repeats the flag {}",
                verb.name(),
                flag.word()
            );
            seen.push(flag.word());
        }
    }
}

/// The representative verbs, one per flag pattern: required value flags,
/// optional ones, repeatable ones, bare switches, and the body sources.
/// The switch verbs ride without their switch in the base line, so the
/// probe value flips the switch and parse reads it as a change.
const DRIFT_SPELLINGS: &[&[&str]] = &[
    &["messages", "list", "--account", "alpha"],
    &["messages", "read", "--account", "alpha", "--key", "k1"],
    &["messages", "search", "--query", "q", "--account", "alpha"],
    &["messages", "mark-read", "--account", "alpha", "--key", "k1"],
    &[
        "messages",
        "set-flagged",
        "--account",
        "alpha",
        "--key",
        "k1",
    ],
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
    &["snippets", "create", "--title", "t", "--body", "b"],
    &["keys", "import", "--account", "alpha", "--plain", "cert"],
    &[
        "keys",
        "promote",
        "--account",
        "alpha",
        "--standard",
        "openpgp",
        "--fingerprint",
        "AA",
    ],
    &[
        "keys",
        "reject",
        "--account",
        "alpha",
        "--id",
        "1",
        "--standard",
        "openpgp",
        "--fingerprint",
        "AA",
    ],
];

/// The closed flag vocabulary the reverse drift probe walks: every flag
/// the descriptors spell. It is pinned as a set by
/// `the_flag_vocabulary_is_the_closed_set`, so a descriptor cannot quietly
/// drop a flag and shrink the probe's reach; the closed set is the
/// reference, the closed action set's own precedent.
const FLAG_VOCABULARY: &[&str] = &[
    "--account",
    "--action",
    "--body",
    "--body-file",
    "--cc",
    "--clear",
    "--condition",
    "--email",
    "--file",
    "--fingerprint",
    "--folder",
    "--from",
    "--id",
    "--key",
    "--limit",
    "--name",
    "--offset",
    "--plain",
    "--query",
    "--standard",
    "--subject",
    "--title",
    "--to",
    "--unread",
    "--verified",
];

#[test]
fn the_flag_vocabulary_is_the_closed_set() {
    let mut union: Vec<&str> = Vec::new();
    for spelling in SPELLINGS {
        for flag in completions::descriptor(&probe(spelling)).flags {
            if !union.contains(&flag.word()) {
                union.push(flag.word());
            }
        }
    }
    union.sort_unstable();
    let mut expected = FLAG_VOCABULARY.to_vec();
    expected.sort_unstable();
    assert_eq!(
        union, expected,
        "the descriptor flag vocabulary must stay the closed set"
    );
}

/// Whether parse consumes one flag on a verb's line: the flag either
/// changes the parsed verb or fails the line with a message naming it.
/// The hand-rolled parser ignores flags it does not read, so an unchanged
/// verb on a successful parse is the ignore signal. A probe value rides
/// after the flag; the representative verbs all ignore stray positionals,
/// so a bare switch reads the same with it present.
fn parse_consumes(base: &[&str], verb: &Verb, flag: &str) -> bool {
    let mut line: Vec<String> = base.iter().map(|word| (*word).to_owned()).collect();
    line.push(flag.to_owned());
    line.push("probe".to_owned());
    match parse::parse(&line, &mut std::io::empty()) {
        Ok(invocation) => &invocation.verb != verb,
        Err(failure) => failure.message.contains(flag),
    }
}

#[test]
fn the_descriptor_matches_parse_for_the_representative_verbs() {
    for words in DRIFT_SPELLINGS {
        let verb = probe(words);
        let entry = completions::descriptor(&verb);
        let spelled: Vec<&str> = entry.flags.iter().map(|flag| flag.word()).collect();
        // Forward: every descriptor flag is one parse consumes.
        for flag in entry.flags {
            assert!(
                parse_consumes(words, &verb, flag.word()),
                "{}: descriptor flag {} is not consumed by parse",
                verb.name(),
                flag.word()
            );
        }
        // Reverse, within the closed flag vocabulary and for these
        // representative verbs only: any vocabulary flag parse consumes
        // here must be spelled in this verb's descriptor, so a flag that
        // moves between verbs, or one the descriptor drops while parse
        // still reads it, fails here. A wholly new flag added to parse
        // that no descriptor spells is outside the closed vocabulary and
        // is not caught by this pin.
        for word in FLAG_VOCABULARY {
            if parse_consumes(words, &verb, word) {
                assert!(
                    spelled.contains(word),
                    "{}: parse consumes {word} but the descriptor omits it",
                    verb.name()
                );
            }
        }
    }
}
