// SPDX-License-Identifier: GPL-3.0-only
//! Dispatch: the plumbing every verb shares. The parser lives in
//! [`parse`], the verbs in their own modules (the dial-nothing provider
//! in [`offline`]), and this module ties one invocation to the engine
//! store, the audit ledger, and the envelope `main` prints.

pub(crate) mod accounts;
#[cfg(test)]
mod accounts_tests;
pub(crate) mod ai;
#[cfg(test)]
mod ai_docs_tests;
#[cfg(test)]
mod ai_replies_tests;
#[cfg(test)]
mod ai_tests;
pub(crate) mod completions;
#[cfg(test)]
mod completions_drift_tests;
#[cfg(test)]
mod completions_tests;
pub(crate) mod drafts;
#[cfg(test)]
mod drafts_tests;
#[cfg(test)]
pub(crate) mod fakes;
pub(crate) mod folders;
#[cfg(test)]
mod folders_tests;
pub(crate) mod keys;
#[cfg(test)]
mod keys_review_tests;
#[cfg(test)]
mod keys_tests;
pub(crate) mod knowledge;
#[cfg(test)]
mod knowledge_tests;
pub(crate) mod message_writes;
#[cfg(test)]
mod message_writes_tests;
pub(crate) mod messages;
#[cfg(test)]
mod messages_tests;
pub(crate) mod offline;
mod parse;
pub(crate) mod rules;
#[cfg(test)]
mod rules_tests;
pub(crate) mod snippets;
#[cfg(test)]
mod snippets_tests;
mod verb;

pub(crate) use offline::Offline;
pub(crate) use verb::Verb;

use std::io::Read;
use std::path::Path;

use astra_ai::AiStore;
use engine_api::{AccountId, Engine};
use serde_json::Value;

use crate::audit;
use crate::envelope::{Code, Envelope};

/// The engine store's file name under the data dir, the same file the desktop
/// host opens (`boot.rs`).
pub(crate) const ENGINE_STORE_FILE: &str = "astra.sqlite";

/// The AI ledger's file name under the data dir.
pub(crate) const AI_STORE_FILE: &str = "ai.sqlite";

/// The send-class words the parser refuses before anything opens: humans
/// decide and send, so no such verb can ever reach the engine.
pub(crate) const SEND_CLASS: &[&str] = &["send", "queue"];

/// The default `messages list` limit, so the verb is usable unadorned.
pub(crate) const DEFAULT_LIMIT: usize = 25;

/// What one finished invocation hands `main`.
pub(crate) struct Run {
    /// The envelope to print.
    pub(crate) envelope: Envelope,
    /// The process exit status.
    pub(crate) code: Code,
    /// Whether `--json` asked for the one-line form.
    pub(crate) json: bool,
    /// The raw script a generator verb prints instead of the envelope when
    /// `--json` is absent; `None` for every other verb.
    pub(crate) raw: Option<String>,
}

/// One parsed invocation.
pub(crate) struct Invocation {
    /// What to run.
    pub(crate) verb: Verb,
    /// Whether `--preview` was passed (mutating verbs only).
    pub(crate) preview: bool,
}

/// `snippets create`'s arguments, with the body text resolved at parse
/// time so the audit preview can count it and execution cannot fail
/// reading it, the drafts precedent.
#[derive(PartialEq)]
pub(crate) struct SnippetArgs {
    /// The snippet's short title, the picker's label.
    pub(crate) title: String,
    /// The snippet's body, the text an insert places at the cursor.
    pub(crate) body: String,
    /// Which source the body came from, for the audit preview.
    pub(crate) source: &'static str,
}

/// `drafts save`'s arguments, with the body text resolved at parse time so
/// the audit preview can count it and execution cannot fail reading it.
#[derive(PartialEq)]
pub(crate) struct DraftArgs {
    /// The account id from the index.
    pub(crate) account: String,
    /// The sender address.
    pub(crate) from: String,
    /// The recipient addresses.
    pub(crate) to: Vec<String>,
    /// The Cc recipient addresses.
    pub(crate) cc: Vec<String>,
    /// The subject.
    pub(crate) subject: String,
    /// The plain-text body.
    pub(crate) body: String,
    /// Which source the body came from, for the audit preview.
    pub(crate) source: &'static str,
}

/// A parse failure: the exit code it implies and the message to carry.
pub(crate) struct ParseFailure {
    /// Usage for a malformed command line; Refused for a send-class verb.
    pub(crate) code: Code,
    /// What was wrong, in words.
    pub(crate) message: String,
}

/// The answer shape every verb hands back: data on success, a code and
/// message on failure.
pub(crate) type Answer = Result<Value, (Code, String)>;

/// Runs one invocation end to end: parse, audit, execute, answer.
pub(crate) fn run(args: &[String], data_dir: &Path, stdin: &mut dyn Read) -> Run {
    let runtime = tokio::runtime::Runtime::new()
        .unwrap_or_else(|err| panic!("the astra-cli runtime could not start: {err}"));
    runtime.block_on(dispatch(args, data_dir, stdin))
}

/// The async half of [`run`], where the engine's reads live.
async fn dispatch(args: &[String], data_dir: &Path, stdin: &mut dyn Read) -> Run {
    let json = parse::flag_present(args, "--json");
    let correlation = parse::correlation_from(args);
    let parsed = parse::parse(args, stdin);
    // The ledger opens for every invocation, whatever the parse said: the
    // audit contract covers failures too. The data dir is made first, the
    // way the app's boot makes it.
    let opened = std::fs::create_dir_all(data_dir)
        .and_then(|()| AiStore::open(&data_dir.join(AI_STORE_FILE)).map_err(std::io::Error::other));
    let store = match opened {
        Ok(store) => store,
        Err(err) => {
            return Run {
                envelope: Envelope::failure(
                    Code::Runtime,
                    format!("the audit ledger could not be opened: {err}"),
                ),
                code: Code::Runtime,
                json,
                raw: None,
            }
        }
    };
    let run = match parsed {
        Ok(ref invocation) => {
            let outcome = execute(invocation, data_dir, &store).await;
            let raw = if json {
                None
            } else {
                raw_script(&invocation.verb, &outcome)
            };
            to_run(outcome, json, raw)
        }
        Err(ref failure) => to_run(Err((failure.code, failure.message.clone())), json, None),
    };
    // Fail closed: an invocation that cannot be audited is not answered,
    // whatever the verb itself achieved.
    let (verb, preview) = match parsed {
        Ok(ref invocation) => (
            invocation.verb.name().to_owned(),
            audit::arg_preview(&invocation.verb),
        ),
        Err(ref failure) => (parse::raw_verb(args, failure.code), String::new()),
    };
    let preview_held = matches!(&parsed, Ok(invocation) if invocation.preview);
    let outcome = if preview_held {
        audit::outcome::PREVIEW
    } else {
        audit_outcome(&run)
    };
    if audit::record(&store, &correlation, &verb, &preview, outcome).is_err() {
        return Run {
            envelope: Envelope::failure(Code::Runtime, "the audit ledger could not be written"),
            code: Code::Runtime,
            json,
            raw: None,
        };
    }
    run
}

/// The raw script a generator verb prints in place of the envelope when
/// `--json` is absent: the completions verb's script, and nothing for any
/// other verb.
fn raw_script(verb: &Verb, outcome: &Answer) -> Option<String> {
    match (verb, outcome) {
        (Verb::Completions { .. }, Ok(data)) => data
            .get("script")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

/// Wraps one verb's answer as the run `main` prints.
fn to_run(outcome: Answer, json: bool, raw: Option<String>) -> Run {
    match outcome {
        Ok(data) => Run {
            envelope: Envelope::success(data),
            code: Code::Ok,
            json,
            raw,
        },
        Err((code, message)) => Run {
            envelope: Envelope::failure(code, message),
            code,
            json,
            raw: None,
        },
    }
}

/// The ledger's outcome word for one finished run.
fn audit_outcome(run: &Run) -> &'static str {
    match run.code {
        Code::Ok => audit::outcome::OK,
        Code::Usage => audit::outcome::USAGE,
        Code::Refused => audit::outcome::REFUSED,
        Code::Runtime => audit::outcome::ERROR,
    }
}

/// Executes one parsed invocation against the data dir. The mail verbs
/// open the engine store; the rules and AI verbs answer straight from the
/// audit ledger's own store, the one file every invocation already
/// opened (the rules and results tables live beside the ledgers).
async fn execute(invocation: &Invocation, data_dir: &Path, store: &AiStore) -> Answer {
    match &invocation.verb {
        Verb::AccountsList => return accounts::list(data_dir),
        Verb::Completions { shell } => {
            return Ok(serde_json::json!({
                "shell": shell.word(),
                "script": completions::script(*shell),
            }));
        }
        Verb::RulesList => return rules::list(store),
        Verb::RulesCreate {
            name,
            condition,
            actions,
        } => return rules::create(store, name, condition, actions, invocation.preview),
        Verb::RulesDelete { id } => return rules::delete(store, id, invocation.preview),
        Verb::KnowledgeList { account } => {
            let account = account_id(data_dir, account)?;
            return knowledge::list(store, &account);
        }
        Verb::KnowledgeForget { id } => return knowledge::forget(store, id, invocation.preview),
        Verb::KnowledgeExtract => return knowledge::extract(),
        Verb::SnippetsList => return snippets::list(store),
        Verb::SnippetsCreate { args } => return snippets::create(store, args, invocation.preview),
        Verb::SnippetsDelete { id } => return snippets::delete(store, id, invocation.preview),
        Verb::SnippetsSearch => return snippets::search(),
        Verb::KeysList { account } => return keys::list(data_dir, &account_id(data_dir, account)?),
        Verb::KeysCollected { account } => {
            return keys::collected(data_dir, &account_id(data_dir, account)?);
        }
        Verb::KeysImport { args } => {
            let account = account_id(data_dir, &args.account)?;
            return keys::import(data_dir, &account, args, invocation.preview);
        }
        Verb::KeysPromote { args } => {
            let account = account_id(data_dir, &args.account)?;
            return keys::promote(data_dir, &account, args, invocation.preview);
        }
        Verb::KeysReject { args } => {
            let account = account_id(data_dir, &args.account)?;
            return keys::reject(data_dir, &account, args, invocation.preview);
        }
        Verb::AiCold { account, keys } => {
            let account = account_id(data_dir, account)?;
            return ai::cold(store, &account, keys);
        }
        Verb::AiResults { account, keys } => {
            let account = account_id(data_dir, account)?;
            return ai::results(store, &account, keys);
        }
        Verb::AiDrafts { account, keys } => {
            let account = account_id(data_dir, account)?;
            return ai::drafts(store, &account, keys);
        }
        Verb::AiSummary { account, keys } => {
            let account = account_id(data_dir, account)?;
            return ai::summary(store, &account, keys);
        }
        Verb::AiStyle { account } => {
            let account = account_id(data_dir, account)?;
            return ai::style(store, &account);
        }
        Verb::AiStyleRebuild => return ai::style_rebuild(),
        Verb::AiDigest { account } => {
            let account = account_id(data_dir, account)?;
            return ai::digest(store, &account);
        }
        Verb::AiReport { account } => {
            let account = account_id(data_dir, account)?;
            return ai::report(store, &account);
        }
        Verb::AiDigestGenerate => return ai::digest_generate(),
        Verb::AiReportGenerate => return ai::report_generate(),
        _ => {}
    }
    let engine = match Engine::open(data_dir.join(ENGINE_STORE_FILE)) {
        Ok(engine) => engine,
        Err(err) => {
            return Err((
                Code::Runtime,
                format!("the engine store could not be opened: {err}"),
            ))
        }
    };
    match &invocation.verb {
        Verb::AccountsList
        | Verb::Completions { .. }
        | Verb::RulesList
        | Verb::RulesCreate { .. }
        | Verb::RulesDelete { .. }
        | Verb::KnowledgeList { .. }
        | Verb::KnowledgeForget { .. }
        | Verb::KnowledgeExtract
        | Verb::KeysList { .. }
        | Verb::KeysCollected { .. }
        | Verb::KeysImport { .. }
        | Verb::KeysPromote { .. }
        | Verb::KeysReject { .. }
        | Verb::SnippetsList
        | Verb::SnippetsCreate { .. }
        | Verb::SnippetsDelete { .. }
        | Verb::SnippetsSearch
        | Verb::AiCold { .. }
        | Verb::AiResults { .. }
        | Verb::AiDrafts { .. }
        | Verb::AiSummary { .. }
        | Verb::AiStyle { .. }
        | Verb::AiStyleRebuild
        | Verb::AiDigest { .. }
        | Verb::AiReport { .. }
        | Verb::AiDigestGenerate
        | Verb::AiReportGenerate => unreachable!("answered before the engine opened"),
        Verb::FoldersList { account } => {
            let account = account_id(data_dir, account)?;
            folders::list(&engine, &account).await
        }
        Verb::MessagesList {
            account,
            folder,
            limit,
        } => {
            let account = account_id(data_dir, account)?;
            messages::list(&engine, &account, folder.as_deref(), *limit).await
        }
        Verb::MessagesRead { account, keys } => {
            let account = account_id(data_dir, account)?;
            messages::read(&engine, &Offline, &account, keys).await
        }
        Verb::MessagesSearch {
            query,
            account,
            folder,
            limit,
            offset,
        } => {
            // The covered set is the whole index unless one account is
            // named, the MCP search contract's own scoping.
            let covered = match account {
                Some(named) => vec![account_id(data_dir, named)?],
                None => index_accounts(data_dir)?,
            };
            let named = account.is_some();
            messages::search(
                &engine,
                &covered,
                named,
                folder.as_deref(),
                query,
                *offset,
                *limit,
            )
            .await
        }
        Verb::MessagesMarkRead {
            account,
            key,
            unread,
        } => {
            let account = account_id(data_dir, account)?;
            message_writes::mark_read(
                &engine,
                &Offline,
                &account,
                key,
                !unread,
                invocation.preview,
            )
            .await
        }
        Verb::MessagesSetFlagged {
            account,
            key,
            clear,
        } => {
            let account = account_id(data_dir, account)?;
            message_writes::set_flagged(
                &engine,
                &Offline,
                &account,
                key,
                !clear,
                invocation.preview,
            )
            .await
        }
        Verb::MessagesArchive { account, key } => {
            let account = account_id(data_dir, account)?;
            message_writes::archive(&engine, &Offline, &account, key, invocation.preview).await
        }
        Verb::MessagesTrash { account, key } => {
            let account = account_id(data_dir, account)?;
            message_writes::trash(&engine, &Offline, &account, key, invocation.preview).await
        }
        Verb::DraftsSave { args } => {
            let account = account_id(data_dir, &args.account)?;
            drafts::save(&engine, &Offline, &account, args, invocation.preview).await
        }
        Verb::DraftsDiscard { account, key } => {
            let account = account_id(data_dir, account)?;
            drafts::discard(&engine, &Offline, &account, key, invocation.preview).await
        }
    }
}

/// Resolves an account id string against the index: known ids become
/// `AccountId`s, unknown ones fail with a runtime error naming the account.
fn account_id(data_dir: &Path, account: &str) -> Result<AccountId, (Code, String)> {
    match accounts::index_ids(data_dir) {
        Ok(ids) if ids.iter().any(|id| id == account) => AccountId::try_from(account)
            .map_err(|err| (Code::Usage, format!("the account id is malformed: {err}"))),
        Ok(_) => Err((
            Code::Runtime,
            format!("unknown account {account}: not in the account index"),
        )),
        Err(err) => Err((Code::Runtime, err)),
    }
}

/// Every account the index names, the set an unscoped search covers. A
/// malformed index id fails loudly rather than silently narrowing the
/// search.
fn index_accounts(data_dir: &Path) -> Result<Vec<AccountId>, (Code, String)> {
    let ids = accounts::index_ids(data_dir).map_err(|err| (Code::Runtime, err))?;
    ids.iter()
        .map(|id| {
            AccountId::try_from(id.as_str()).map_err(|err| {
                (
                    Code::Runtime,
                    format!("the account index holds a malformed id {id}: {err}"),
                )
            })
        })
        .collect()
}
