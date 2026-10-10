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
        Verb::AccountsList => String::new(),
        Verb::FoldersList { account } => format!("account={account}"),
        Verb::MessagesList {
            account,
            folder,
            limit,
        } => match folder {
            Some(folder) => format!("account={account} folder={folder} limit={limit}"),
            None => format!("account={account} limit={limit}"),
        },
        Verb::MessagesRead { account, keys } => {
            format!("account={account} keys={}", keys.len())
        }
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
    }
}
