// SPDX-License-Identifier: GPL-3.0-only
//! The per-invocation audit sink: every `astra-cli` run, successful or not,
//! lands one row in the AI ledger beside the data dir, tied to the turn's
//! correlation id (the spec's traceability contract). The argument preview is
//! redacted by construction: it carries shapes (counts, lengths, store ids),
//! never message text, subjects, or addresses.

use astra_ai::{AiStore, AuditRow, CorrelationId};

use crate::verbs::Verb;

/// The outcome vocabulary the ledger's `outcome` column carries.
pub(crate) mod outcome {
    /// The verb ran and succeeded.
    pub(crate) const OK: &str = "ok";
    /// The verb ran and failed at runtime.
    pub(crate) const ERROR: &str = "error";
    /// The arguments did not parse.
    pub(crate) const USAGE: &str = "usage";
    /// A send-class verb, parsed and refused.
    pub(crate) const REFUSED: &str = "refused";
    /// A mutating verb answered with its preview instead of acting.
    pub(crate) const PREVIEW: &str = "preview";
}

/// Records one invocation.
///
/// # Errors
/// Fails when the ledger insert cannot run; the caller fails closed rather
/// than answering an unaudited invocation.
pub(crate) fn record(
    store: &AiStore,
    correlation: &CorrelationId,
    verb: &str,
    arg_preview: &str,
    outcome: &str,
) -> Result<(), astra_ai::AiStoreError> {
    store.record_audit(&AuditRow {
        correlation: correlation.clone(),
        verb: verb.to_owned(),
        arg_preview: arg_preview.to_owned(),
        outcome: outcome.to_owned(),
    })
}

/// The redacted argument preview for one parsed verb: shapes and store ids
/// only, so the ledger can say what was asked without ever holding what was
/// said. Address counts and text lengths stand in for the values.
pub(crate) fn arg_preview(verb: &Verb) -> String {
    match verb {
        // The account-only previews and the empty ones: the account lists
        // (folder, style, knowledge) carry the account alone, the
        // no-argument lists and the refused spellings (no account
        // resolves, no action runs) carry nothing.
        Verb::FoldersList { account }
        | Verb::AiStyle { account }
        | Verb::KnowledgeList { account } => format!("account={account}"),
        Verb::AccountsList
        | Verb::RulesList
        | Verb::AiStyleRebuild
        | Verb::KnowledgeExtract
        | Verb::SnippetsList
        | Verb::SnippetsSearch => String::new(),
        // Every id-carrying verb previews the id alone.
        Verb::RulesDelete { id } | Verb::KnowledgeForget { id } | Verb::SnippetsDelete { id } => {
            format!("id={id}")
        }
        Verb::SnippetsCreate { args } => format!(
            "title_chars={} body_chars={} source={}",
            args.title.chars().count(),
            args.body.chars().count(),
            args.source,
        ),
        Verb::MessagesList {
            account,
            folder,
            limit,
        } => match folder {
            Some(folder) => format!("account={account} folder={folder} limit={limit}"),
            None => format!("account={account} limit={limit}"),
        },
        // Every key-carrying read previews the same shape: the account and
        // the count, never the caller's key words.
        Verb::MessagesRead { account, keys }
        | Verb::AiCold { account, keys }
        | Verb::AiResults { account, keys }
        | Verb::AiDrafts { account, keys }
        | Verb::AiSummary { account, keys } => format!("account={account} keys={}", keys.len()),
        Verb::DraftsSave { args } => format!(
            "account={} from=1 to={} cc={} subject_chars={} body_chars={} source={}",
            args.account,
            args.to.len(),
            args.cc.len(),
            args.subject.chars().count(),
            args.body.chars().count(),
            args.source,
        ),
        Verb::DraftsDiscard { account, key } => format!("account={account} key={key}"),
        Verb::RulesCreate {
            name,
            condition,
            actions,
        } => format!(
            "name_chars={} condition_chars={} actions={}",
            name.chars().count(),
            condition.chars().count(),
            actions.len(),
        ),
    }
}
