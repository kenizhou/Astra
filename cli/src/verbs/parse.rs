// SPDX-License-Identifier: GPL-3.0-only
//! The command-line parser: two positionals (`verb subverb`) plus flags, the
//! drafts body resolved from its one source, and the send-class refusal that
//! lives here so no such spelling can ever reach the engine.

use std::io::Read;
use std::path::Path;

use astra_ai::CorrelationId;

use super::{DraftArgs, Invocation, ParseFailure, Verb, DEFAULT_LIMIT, SEND_CLASS};
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
