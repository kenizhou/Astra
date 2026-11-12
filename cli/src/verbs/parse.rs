// SPDX-License-Identifier: GPL-3.0-only
//! The command-line parser: two positionals (`verb subverb`) plus flags, the
//! drafts body resolved from its one source, and the send-class refusal that
//! lives here so no such spelling can ever reach the engine.

use std::io::Read;
use std::path::Path;

use astra_ai::CorrelationId;

use super::ai;
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
        (Some("messages"), Some("search")) => {
            let account = owned_value(args, "--account")?;
            let folder = owned_value(args, "--folder")?;
            if account.is_none() && folder.is_some() {
                return Err(usage(
                    "the --folder narrows within one account, so it requires --account too",
                ));
            }
            let limit = match one_value(args, "--limit")? {
                Some(word) => word
                    .parse::<usize>()
                    .map_err(|_| usage("the --limit flag wants a number from 1 to 50"))?,
                None => super::messages::DEFAULT_SEARCH_LIMIT,
            };
            if !(1..=super::messages::MAX_SEARCH_LIMIT).contains(&limit) {
                return Err(usage("the --limit flag wants a number from 1 to 50"));
            }
            let offset = match one_value(args, "--offset")? {
                Some(word) => word
                    .parse::<usize>()
                    .map_err(|_| usage("the --offset flag wants a number"))?,
                None => 0,
            };
            Verb::MessagesSearch {
                query: non_empty(required(args, "--query")?, "--query")?,
                account,
                folder,
                limit,
                offset,
            }
        }
        (Some("messages"), Some("mark-read")) => Verb::MessagesMarkRead {
            account: required(args, "--account")?,
            key: required(args, "--key")?,
            unread: flag_present(args, "--unread"),
        },
        (Some("messages"), Some("set-flagged")) => Verb::MessagesSetFlagged {
            account: required(args, "--account")?,
            key: required(args, "--key")?,
            clear: flag_present(args, "--clear"),
        },
        (Some("messages"), Some("archive")) => Verb::MessagesArchive {
            account: required(args, "--account")?,
            key: required(args, "--key")?,
        },
        (Some("messages"), Some("trash")) => Verb::MessagesTrash {
            account: required(args, "--account")?,
            key: required(args, "--key")?,
        },
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
            let ai_words = ai::ai_positionals(args);
            Verb::AiCold {
                account: ai::positional_account(&ai_words, "ai cold")?,
                keys: ai::positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("results")) => {
            let ai_words = ai::ai_positionals(args);
            Verb::AiResults {
                account: ai::positional_account(&ai_words, "ai results")?,
                keys: ai::positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("drafts")) => {
            let ai_words = ai::ai_positionals(args);
            Verb::AiDrafts {
                account: ai::positional_account(&ai_words, "ai drafts")?,
                keys: ai::positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("summary")) => {
            let ai_words = ai::ai_positionals(args);
            Verb::AiSummary {
                account: ai::positional_account(&ai_words, "ai summary")?,
                keys: ai::positional_keys(&ai_words)?,
            }
        }
        (Some("ai"), Some("style")) => ai::style_verb(&ai::ai_positionals(args))?,
        (Some("ai"), Some(sub @ ("digest" | "report"))) => {
            ai::document_verb(&ai::ai_positionals(args), sub)?
        }
        (Some("completions"), Some(word)) => Verb::Completions {
            shell: super::completions::Shell::from_word(word).ok_or_else(|| {
                usage(&format!(
                    "completions wants a shell: {}",
                    super::completions::shell_words()
                ))
            })?,
        },
        (Some("completions"), None) => {
            return Err(usage(&format!(
                "completions wants a shell: {}",
                super::completions::shell_words()
            )));
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
pub(super) fn usage(message: &str) -> ParseFailure {
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

/// The second positionals the verbs spell: the subverbs and the
/// completion shells, the CLI's own vocabulary, the only second
/// positional the ledger may capture beside a verb word.
const SUBVERBS: &[&str] = &[
    "list",
    "read",
    "save",
    "discard",
    "create",
    "delete",
    "cold",
    "results",
    "drafts",
    "summary",
    "style",
    "digest",
    "report",
    "forget",
    "extract",
    "search",
    "mark-read",
    "set-flagged",
    "archive",
    "trash",
    "bash",
    "zsh",
    "fish",
    "pwsh",
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
