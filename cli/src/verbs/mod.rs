// SPDX-License-Identifier: GPL-3.0-only
//! Dispatch and the offline provider: the plumbing every verb shares. The
//! parser lives in [`parse`], the verbs in their own modules, and this module
//! ties one invocation to the engine store, the audit ledger, and the
//! envelope `main` prints.

pub(crate) mod accounts;
#[cfg(test)]
mod accounts_tests;
pub(crate) mod drafts;
#[cfg(test)]
mod drafts_tests;
#[cfg(test)]
pub(crate) mod fakes;
pub(crate) mod folders;
#[cfg(test)]
mod folders_tests;
pub(crate) mod messages;
#[cfg(test)]
mod messages_tests;
mod parse;

use std::io::Read;
use std::path::Path;

use astra_ai::AiStore;
use async_trait::async_trait;
use engine_api::{AccountId, CalendarWrites, Capabilities, Engine, MailboxWrites, Provider};
use engine_core::sync::{SyncState, SyncWindow};
use engine_provider::{ConnectionInfo, EmailStream, ProviderError};
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

/// The words every offline provider failure carries. Classified retryable so
/// the outbox parks the write for the app's drain rather than settling it as
/// a permanent failure.
const OFFLINE: &str = "the astra-cli dials no server; the app's outbox executes this write";

/// What one finished invocation hands `main`.
pub(crate) struct Run {
    /// The envelope to print.
    pub(crate) envelope: Envelope,
    /// The process exit status.
    pub(crate) code: Code,
    /// Whether `--json` asked for the one-line form.
    pub(crate) json: bool,
}

/// One parsed invocation.
pub(crate) struct Invocation {
    /// What to run.
    pub(crate) verb: Verb,
    /// Whether `--preview` was passed (mutating verbs only).
    pub(crate) preview: bool,
}

/// The six verbs, with their arguments.
pub(crate) enum Verb {
    /// List the account index's ids.
    AccountsList,
    /// List one account's folders.
    FoldersList {
        /// The account id from the index.
        account: String,
    },
    /// List messages, newest first, optionally one folder's.
    MessagesList {
        /// The account id from the index.
        account: String,
        /// The folder's mailbox key, when one folder is asked for.
        folder: Option<String>,
        /// How many rows to answer with.
        limit: usize,
    },
    /// Read whole messages by provider key.
    MessagesRead {
        /// The account id from the index.
        account: String,
        /// The provider keys to resolve.
        keys: Vec<String>,
    },
    /// Store a draft through the engine's outbox.
    DraftsSave {
        /// The assembled arguments, body text included.
        args: DraftArgs,
    },
    /// Remove a stored draft through the engine's outbox.
    DraftsDiscard {
        /// The account id from the index.
        account: String,
        /// The stored draft's provider key.
        key: String,
    },
}

impl Verb {
    /// The ledger's verb spelling.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::AccountsList => "accounts list",
            Self::FoldersList { .. } => "folders list",
            Self::MessagesList { .. } => "messages list",
            Self::MessagesRead { .. } => "messages read",
            Self::DraftsSave { .. } => "drafts save",
            Self::DraftsDiscard { .. } => "drafts discard",
        }
    }

    /// Whether this verb changes anything, and so ships `--preview`.
    pub(crate) fn mutating(&self) -> bool {
        matches!(self, Self::DraftsSave { .. } | Self::DraftsDiscard { .. })
    }
}

/// `drafts save`'s arguments, with the body text resolved at parse time so
/// the audit preview can count it and execution cannot fail reading it.
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
            }
        }
    };
    let run = match parsed {
        Ok(ref invocation) => {
            let outcome = execute(invocation, data_dir).await;
            to_run(outcome, json)
        }
        Err(ref failure) => to_run(Err((failure.code, failure.message.clone())), json),
    };
    // Fail closed: an invocation that cannot be audited is not answered,
    // whatever the verb itself achieved.
    let (verb, preview) = match parsed {
        Ok(ref invocation) => (
            invocation.verb.name().to_owned(),
            audit::arg_preview(&invocation.verb),
        ),
        Err(ref failure) => (raw_verb(args, failure.code), String::new()),
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
        };
    }
    run
}

/// Wraps one verb's answer as the run `main` prints.
fn to_run(outcome: Answer, json: bool) -> Run {
    match outcome {
        Ok(data) => Run {
            envelope: Envelope::success(data),
            code: Code::Ok,
            json,
        },
        Err((code, message)) => Run {
            envelope: Envelope::failure(code, message),
            code,
            json,
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

/// The verb words of an unparsable line, best effort: two positionals, one,
/// or a dash for none. A refused send-class line records its single verb
/// word alone; its own arguments are not the verb.
fn raw_verb(args: &[String], code: Code) -> String {
    let positionals = parse::positional_words(args);
    match (positionals.len(), code) {
        (0, _) => "-".to_owned(),
        (1, _) | (_, Code::Refused) => positionals[0].to_owned(),
        _ => format!("{} {}", positionals[0], positionals[1]),
    }
}

/// Executes one parsed invocation against the data dir.
async fn execute(invocation: &Invocation, data_dir: &Path) -> Answer {
    if let Verb::AccountsList = invocation.verb {
        return accounts::list(data_dir);
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
        Verb::AccountsList => unreachable!("answered before the engine opened"),
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

/// The provider every CLI write and body fetch goes through: it dials
/// nothing. Draft writes park in the engine's outbox for the app's drain, and
/// an uncached body is an error rather than a network fetch.
pub(crate) struct Offline;

#[async_trait]
impl Provider for Offline {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none())
    }

    fn stream_email<'a>(
        &'a self,
        account: &'a AccountId,
        cursor: Option<&'a SyncState>,
        _window: SyncWindow,
        _fetch_batch: usize,
        _chunk_size: usize,
    ) -> EmailStream<'a> {
        let _ = (account, cursor);
        Box::pin(futures::stream::once(async {
            Err(ProviderError::retryable(OFFLINE))
        }))
    }

    async fn fetch_message_source(
        &self,
        _account: &AccountId,
        _message: &engine_api::Message,
    ) -> Result<engine_core::raw::RawMime, engine_provider::ProviderError> {
        Err(ProviderError::retryable(OFFLINE))
    }

    async fn put_draft(
        &self,
        _account: &AccountId,
        _draft: &engine_api::Draft,
        _replacing: Option<&engine_api::ProviderKey>,
    ) -> Result<engine_api::ProviderKey, engine_provider::ProviderError> {
        Err(ProviderError::retryable(OFFLINE))
    }

    async fn delete_draft(
        &self,
        _account: &AccountId,
        _draft: &engine_api::ProviderKey,
    ) -> Result<(), engine_provider::ProviderError> {
        Err(ProviderError::retryable(OFFLINE))
    }
}

impl MailboxWrites for Offline {}
impl CalendarWrites for Offline {}
