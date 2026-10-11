// SPDX-License-Identifier: GPL-3.0-only
//! The command-line parser: two positionals (`verb subverb`) plus flags, the
//! drafts body resolved from its one source, and the send-class refusal that
//! lives here so no such spelling can ever reach the engine.

use std::io::Read;
use std::path::Path;

use astra_ai::CorrelationId;

use super::{DraftArgs, Invocation, ParseFailure, SnippetArgs, Verb, DEFAULT_LIMIT, SEND_CLASS};
use crate::envelope::Code;

/// The refusal's message, naming the rule it enforces.
const SEND_REFUSAL: &str =
    "sending is a human action: the app sends; the astra-cli refuses send-class verbs";

/// Parses one command line into an invocation, resolving the drafts body
/// from its source. Send-class verbs parse into a refusal, not an error:
/// the parser is where the human-send rule is enforced.
pub(super) fn parse(args: &[String], stdin: &mut dyn Read) -> Result<Invocation, ParseFailure> {
    let positionals = positional_words(args);
    let first = positionals.first().copied();
    if let Some(word) = first {
        if SEND_CLASS.contains(&word) {
            return Err(ParseFailure {
                code: Code::Refused,
                message: SEND_REFUSAL.to_owned(),
            });
        }
    }
    let sub = positionals.get(1).copied();
    let verb = match (first, sub) {
        (Some("accounts"), Some("list")) => Verb::AccountsList,
        (Some("folders"), Some("list")) => Verb::FoldersList {
            account: required(args, "--account")?,
        },
        (Some("messages"), Some("list")) => Verb::MessagesList {
            account: required(args, "--account")?,
            folder: owned_value(args, "--folder")?,
            limit: match one_value(args, "--limit")? {
                Some(word) => word
                    .parse::<usize>()
                    .map_err(|_| usage("--limit wants a number"))?,
                None => DEFAULT_LIMIT,
            },
        },
        (Some("messages"), Some("read")) => {
            let keys = many_values(args, "--key");
            if keys.is_empty() {
                return Err(usage("messages read wants at least one --key"));
            }
            Verb::MessagesRead {
                account: required(args, "--account")?,
                keys,
            }
        }
        (Some("drafts"), Some("save")) => Verb::DraftsSave {
            args: draft_args(args, stdin)?,
        },
        (Some("drafts"), Some("discard")) => Verb::DraftsDiscard {
            account: required(args, "--account")?,
            key: required(args, "--key")?,
        },
        (Some("rules"), Some("list")) => Verb::RulesList,
        (Some("rules"), Some("create")) => Verb::RulesCreate {
            name: non_empty(required(args, "--name")?, "--name")?,
            condition: non_empty(required(args, "--condition")?, "--condition")?,
            actions: rule_actions(args)?,
        },
        (Some("rules"), Some("delete")) => Verb::RulesDelete {
            id: required(args, "--id")?,
        },
        (Some("knowledge"), Some("list")) => Verb::KnowledgeList {
            account: required(args, "--account")?,
        },
        (Some("knowledge"), Some("forget")) => Verb::KnowledgeForget {
            id: required(args, "--id")?,
        },
        // The never-dials rule's edge: the spelling parses whatever else
        // the line carried, so the refusal (not a usage error) answers a
        // caller that reached for the model-backed run.
        (Some("knowledge"), Some("extract")) => Verb::KnowledgeExtract,
        (Some("snippets"), Some("list")) => Verb::SnippetsList,
        (Some("snippets"), Some("create")) => Verb::SnippetsCreate {
            args: snippet_args(args)?,
        },
        (Some("snippets"), Some("delete")) => Verb::SnippetsDelete {
            id: required(args, "--id")?,
        },
        (Some("snippets"), Some("search")) => Verb::SnippetsSearch,
        (Some("ai"), Some("cold")) => {
            let ai_words = ai_positionals(args);
            Verb::AiCold {
                account: positional_account(&ai_words, "ai cold")?,
                keys: positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("results")) => {
            let ai_words = ai_positionals(args);
            Verb::AiResults {
                account: positional_account(&ai_words, "ai results")?,
                keys: positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("drafts")) => {
            let ai_words = ai_positionals(args);
            Verb::AiDrafts {
                account: positional_account(&ai_words, "ai drafts")?,
                keys: positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("summary")) => {
            let ai_words = ai_positionals(args);
            Verb::AiSummary {
                account: positional_account(&ai_words, "ai summary")?,
                keys: positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("style")) => style_verb(&ai_positionals(args))?,
        (Some("ai"), Some(sub @ ("digest" | "report"))) => {
            document_verb(&ai_positionals(args), sub)?
        }
        (Some(_), Some(_)) => return Err(usage("unknown verb")),
        _ => {
            return Err(usage(
                "a verb and a subverb are required, for example `astra-cli accounts list`",
            ))
        }
    };
    let preview = flag_present(args, "--preview");
    if preview && !verb.mutating() {
        return Err(usage("only the mutating verbs ship --preview"));
    }
    Ok(Invocation { verb, preview })
}

/// Reads `drafts save`'s arguments, resolving the body from exactly one of
/// `--plain`, `--file`, or stdin.
fn draft_args(args: &[String], stdin: &mut dyn Read) -> Result<DraftArgs, ParseFailure> {
    let account = required(args, "--account")?;
    let from = required(args, "--from")?;
    let subject = required(args, "--subject")?;
    let to = split_addresses(&required(args, "--to")?);
    if to.is_empty() {
        return Err(usage("--to wants at least one address"));
    }
    let cc = split_addresses(&owned_value(args, "--cc")?.unwrap_or_default());
    let (body, source) = match (one_value(args, "--plain")?, one_value(args, "--file")?) {
        (Some(_), Some(_)) => {
            return Err(usage(
                "the body comes from one source: --plain, --file, or stdin",
            ))
        }
        (Some(text), None) => ((*text).to_owned(), "arg"),
        (None, Some(path)) => (
            std::fs::read_to_string(Path::new(path))
                .map_err(|err| usage(&format!("the --file body could not be read: {err}")))?,
            "file",
        ),
        (None, None) => {
            let mut text = String::new();
            stdin
                .read_to_string(&mut text)
                .map_err(|err| usage(&format!("the stdin body could not be read: {err}")))?;
            (text, "stdin")
        }
    };
    Ok(DraftArgs {
        account,
        from,
        to,
        cc,
        subject,
        body,
        source,
    })
}

/// Reads `snippets create`'s arguments, resolving the body from exactly
/// one of `--body` or `--body-file`, the drafts one-source discipline
/// over the snippet's own pair.
fn snippet_args(args: &[String]) -> Result<SnippetArgs, ParseFailure> {
    let title = non_empty(required(args, "--title")?, "--title")?;
    let file = owned_value(args, "--body-file")?;
    let resolved = match (one_value(args, "--body")?, file) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(usage(
                "the body comes from one source: --body or --body-file",
            ))
        }
        (Some(text), None) => SnippetArgs {
            title,
            body: non_empty((*text).to_owned(), "--body")?,
            source: "arg",
        },
        (None, Some(path)) => SnippetArgs {
            title,
            body: non_empty(
                std::fs::read_to_string(Path::new(&path)).map_err(|err| {
                    usage(&format!("the --body-file body could not be read: {err}"))
                })?,
                "--body-file",
            )?,
            source: "file",
        },
    };
    Ok(resolved)
}

/// A flag that must carry a value, read as an owned `String`.
fn required(args: &[String], flag: &str) -> Result<String, ParseFailure> {
    owned_value(args, flag)?.ok_or_else(|| usage(&missing(flag)))
}

/// Splits a comma-separated address list, dropping empties.
fn split_addresses(field: &str) -> Vec<String> {
    field
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A usage failure's constructor.
fn usage(message: &str) -> ParseFailure {
    ParseFailure {
        code: Code::Usage,
        message: message.to_owned(),
    }
}

/// A flag's value that must carry content, not just characters: a rule's
/// name and condition are data the pass judges by, so a blank one never
/// reaches the store.
fn non_empty(value: String, flag: &str) -> Result<String, ParseFailure> {
    if value.trim().is_empty() {
        return Err(usage(&format!(
            "the {flag} flag wants content, not just whitespace"
        )));
    }
    Ok(value)
}

/// `rules create`'s actions: at least one `--action`, every word from the
/// closed set, no word twice. The closed set is the rules verb's own
/// vocabulary, spelled there beside the verbs that carry it.
fn rule_actions(args: &[String]) -> Result<Vec<String>, ParseFailure> {
    let actions = many_values(args, "--action");
    if actions.is_empty() {
        return Err(usage(
            "rules create wants at least one --action from the closed set",
        ));
    }
    let closed = super::rules::CLOSED_ACTIONS;
    for action in &actions {
        if !closed.contains(&action.as_str()) {
            return Err(usage(&format!(
                "unknown --action {action}; the closed set is {}",
                closed.join(" and ")
            )));
        }
    }
    if actions
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != actions.len()
    {
        return Err(usage(
            "each --action may appear once; a rule's actions are a set",
        ));
    }
    Ok(actions)
}

/// `ai style`'s grammar: one account, or the refused `rebuild` spelling
/// alone. Any further word is a usage error, so the tight grammar every
/// ai verb carries holds for the style pair too.
fn style_verb(positionals: &[&str]) -> Result<Verb, ParseFailure> {
    match (positionals.get(2).copied(), positionals.len()) {
        (None, _) => Err(usage("ai style wants an account (or `rebuild`)")),
        (Some("rebuild"), 3) => Ok(Verb::AiStyleRebuild),
        (Some("rebuild"), _) => Err(usage(
            "ai style rebuild wants no further words: the verb refuses whatever follows",
        )),
        (Some(account), 3) => Ok(Verb::AiStyle {
            account: (*account).to_owned(),
        }),
        (Some(_), _) => Err(usage("ai style takes one account and no keys")),
    }
}

/// `ai digest` and `ai report`'s grammar: one account, or the refused
/// `generate` spelling alone, the style pair's own shape (the account
/// read beside the model work's refusal). Any further word is a usage
/// error, so the tight grammar every ai verb carries holds here too.
fn document_verb(positionals: &[&str], sub: &str) -> Result<Verb, ParseFailure> {
    match (positionals.get(2).copied(), positionals.len()) {
        (None, _) => Err(usage(&format!("ai {sub} wants an account (or `generate`)"))),
        (Some("generate"), 3) => Ok(if sub == "digest" {
            Verb::AiDigestGenerate
        } else {
            Verb::AiReportGenerate
        }),
        (Some("generate"), _) => Err(usage(&format!(
            "ai {sub} generate wants no further words: the verb refuses whatever follows"
        ))),
        (Some(account), 3) => Ok(if sub == "digest" {
            Verb::AiDigest {
                account: (*account).to_owned(),
            }
        } else {
            Verb::AiReport {
                account: (*account).to_owned(),
            }
        }),
        (Some(_), _) => Err(usage(&format!("ai {sub} takes one account and no keys"))),
    }
}

/// The account positional the `ai` verbs take: the third word, after the
/// verb and its subverb.
fn positional_account(positionals: &[&str], verb: &str) -> Result<String, ParseFailure> {
    positionals
        .get(2)
        .map(|word| (*word).to_owned())
        .ok_or_else(|| usage(&format!("{verb} wants an account and at least one key")))
}

/// The key positionals the `ai` verbs take: every word after the account,
/// at least one.
fn positional_keys(positionals: &[&str]) -> Result<Vec<String>, ParseFailure> {
    let keys = positionals
        .iter()
        .skip(3)
        .map(|word| (*word).to_owned())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        return Err(usage("at least one key is required"));
    }
    Ok(keys)
}

/// The positional words of an `ai` line, flag values excluded. Unlike the
/// mail verbs, whose arguments all ride named flags, the ai verbs take
/// their account and keys as bare positionals, so the value of a
/// value-taking flag (`--correlation-id` is the one the CLI defines)
/// must not leak into them.
fn ai_positionals(args: &[String]) -> Vec<&str> {
    let mut words = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--correlation-id" {
            index += 2;
            continue;
        }
        if !args[index].starts_with('-') {
            words.push(args[index].as_str());
        }
        index += 1;
    }
    words
}

/// The missing-flag message.
fn missing(flag: &str) -> String {
    format!("the {flag} flag is required")
}

/// The positional (non-flag) words of a command line.
pub(super) fn positional_words(args: &[String]) -> Vec<&str> {
    args.iter()
        .filter(|word| !word.starts_with('-'))
        .map(String::as_str)
        .collect()
}

/// Whether a bare flag is present.
pub(super) fn flag_present(args: &[String], flag: &str) -> bool {
    args.iter().any(|word| word == flag)
}

/// A flag that takes a value, singular: the last occurrence wins and a value
/// must follow.
fn one_value<'a>(args: &'a [String], flag: &str) -> Result<Option<&'a str>, ParseFailure> {
    let mut seen: Option<&str> = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == flag {
            let value = args.get(index + 1).ok_or_else(|| usage(&missing(flag)))?;
            if value.starts_with('-') {
                return Err(usage(&missing(flag)));
            }
            seen = Some(value);
        }
        index += 1;
    }
    Ok(seen)
}

/// [`one_value`](fn@one_value) as an owned `String`.
fn owned_value(args: &[String], flag: &str) -> Result<Option<String>, ParseFailure> {
    Ok(one_value(args, flag)?.map(str::to_owned))
}

/// A flag that takes a value and may repeat, collecting every occurrence.
fn many_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0;
    while index + 1 < args.len() {
        if args[index] == flag && !args[index + 1].starts_with('-') {
            values.push(args[index + 1].clone());
        }
        index += 1;
    }
    values
}

/// The invocation's correlation id: `--correlation-id`'s value when the line
/// carries one (even a line that fails to parse elsewhere), a fresh v4 UUID
/// otherwise.
pub(super) fn correlation_from(args: &[String]) -> CorrelationId {
    let mut index = 0;
    while index + 1 < args.len() {
        if args[index] == "--correlation-id" && !args[index + 1].starts_with('-') {
            return CorrelationId::new(args[index + 1].clone());
        }
        index += 1;
    }
    CorrelationId::generate()
}

/// The subverbs the verbs spell: the CLI's own vocabulary, the only
/// second positional the ledger may capture beside a verb word.
const SUBVERBS: &[&str] = &[
    "list", "read", "save", "discard", "create", "delete", "cold", "results", "drafts", "summary",
    "style", "digest", "report", "forget", "extract", "search",
];

/// The verb word of an unparsable line, best effort: the first positional,
/// joined with the subverb only when the subverb is the CLI's own
/// vocabulary. A caller-typed positional never reaches the ledger (the
/// correlation id already ties the whole line to its turn), and a refused
/// send-class line records its single verb word alone.
pub(super) fn raw_verb(args: &[String], code: Code) -> String {
    let positionals = positional_words(args);
    let Some(word) = positionals.first().copied() else {
        return "-".to_owned();
    };
    if code == Code::Refused {
        return word.to_owned();
    }
    match positionals.get(1) {
        Some(&sub) if SUBVERBS.contains(&sub) => format!("{word} {sub}"),
        _ => word.to_owned(),
    }
}
