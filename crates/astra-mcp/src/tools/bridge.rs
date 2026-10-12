// SPDX-License-Identifier: GPL-3.0-only
//! The bridge: the input types on one side, the query layer and the write
//! methods on the other. Every read goes through a `query_*` call that
//! changes nothing; every write goes through the same method the UI's
//! intent dispatches to; `create_draft` opens the composer through the
//! compose-request surface and nothing sends.
//!
//! The body rules hold on every result here: `get_message` fences the plain
//! text, listings and searches carry no bodies, and attachments never carry
//! bytes.

use astra_app::{MailWriteError, MessageDetail, MessagePage, OpenDraftError, QueryRow};
use serde_json::{json, Value};

use crate::backend::{AgentDraft, MailBackend};
use crate::fence;
use crate::server::Gate;
use crate::tools::{
    AccountArgs, DraftArgs, ListMessagesArgs, MarkReadArgs, MessageArgs, SearchArgs, SetFlaggedArgs,
};

/// The page size when a client names none: the schema's own default.
const DEFAULT_PAGE: usize = 20;

/// Why a tool call did not produce a result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ToolFailure {
    /// No tool by that name exists.
    #[error("no tool named {0}")]
    Unknown(String),
    /// The arguments did not match the tool's published schema.
    #[error("{0}")]
    BadArgs(String),
    /// The tool exists and the arguments parsed, but the gate or the mail
    /// layer said no. Reported to the model as a tool-level error, because
    /// it is an outcome the assistant should relay and possibly work
    /// around, not a bug.
    #[error("{0}")]
    Refused(String),
}

/// Runs one tool call.
///
/// # Errors
///
/// See [`ToolFailure`].
pub(crate) async fn call(
    backend: &dyn MailBackend,
    gate: &Gate,
    name: &str,
    args: Value,
) -> Result<Value, ToolFailure> {
    // A tool that is not in the catalogue is not callable, even by a client
    // that remembers its name.
    if !crate::tools::tools().iter().any(|tool| tool.name == name) {
        return Err(ToolFailure::Unknown(name.to_owned()));
    }
    match name {
        "list_accounts" => list_accounts(backend, gate).await,
        "list_folders" => list_folders(backend, gate, parse(args)?).await,
        "list_messages" => list_messages(backend, gate, parse(args)?).await,
        "search_messages" => search_messages(backend, gate, parse(args)?).await,
        "get_message" => get_message(backend, gate, parse(args)?).await,
        "mark_read" => mark_read(backend, gate, parse(args)?).await,
        "set_flagged" => set_flagged(backend, gate, parse(args)?).await,
        "archive_message" => {
            let args: MessageArgs = parse(args)?;
            require_allowed(gate, &args.account)?;
            backend
                .archive(&args.account, &args.key)
                .await
                .map_err(write_failure)?;
            Ok(done(&args.account, Some(&args.key), "archived"))
        }
        "move_to_trash" => {
            let args: MessageArgs = parse(args)?;
            require_allowed(gate, &args.account)?;
            backend
                .trash(&args.account, &args.key)
                .await
                .map_err(write_failure)?;
            Ok(done(&args.account, Some(&args.key), "moved to trash"))
        }
        "create_draft" => create_draft(backend, gate, parse(args)?).await,
        _ => Err(ToolFailure::Unknown(name.to_owned())),
    }
}

/// Deserialises a tool's arguments against the very type its published
/// schema agrees with, so both halves of the surface parse identically.
fn parse<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, ToolFailure> {
    serde_json::from_value(args).map_err(|err| ToolFailure::BadArgs(err.to_string()))
}

/// Refuses unless the user exposed `account`.
///
/// The refusal deliberately does **not** distinguish "you did not expose
/// that account" from "there is no such account": which other mailboxes
/// exist is itself something the user did not agree to disclose.
fn require_allowed(gate: &Gate, account: &str) -> Result<(), ToolFailure> {
    if gate.allows(account) {
        return Ok(());
    }
    Err(ToolFailure::Refused(format!(
        "account \"{account}\" is not available to assistants. The user chooses which \
         accounts to expose in the app's advanced settings; by default none are."
    )))
}

/// The successful outcome shape every write answers.
fn done(account: &str, key: Option<&str>, outcome: &str) -> Value {
    let mut result = json!({"account": account, "outcome": outcome});
    if let Some(key) = key {
        result["key"] = json!(key);
    }
    result
}

/// Renders a mail-write failure as the refusal the model can relay.
fn write_failure(error: MailWriteError) -> ToolFailure {
    ToolFailure::Refused(error.to_string())
}

async fn list_accounts(backend: &dyn MailBackend, gate: &Gate) -> Result<Value, ToolFailure> {
    let accounts: Vec<Value> = backend
        .accounts()
        .await
        .into_iter()
        .filter(|account| gate.allows(account.id.as_str()))
        .map(|account| json!({"id": account.id.as_str(), "address": account.address}))
        .collect();
    Ok(json!({"accounts": accounts}))
}

async fn list_folders(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: AccountArgs,
) -> Result<Value, ToolFailure> {
    require_allowed(gate, &args.account)?;
    let folders: Vec<Value> = backend
        .folders(&args.account)
        .await
        .into_iter()
        .map(|folder| {
            json!({
                "key": folder.key.as_str(),
                "name": folder.name,
                "role": folder.role.map(role_word),
                "unread": folder.unread,
            })
        })
        .collect();
    Ok(json!({"account": args.account, "folders": folders}))
}

/// The role as a lowercase word a model can reason over.
fn role_word(role: astra_viewmodel::FolderRole) -> String {
    match role {
        astra_viewmodel::FolderRole::Inbox => "inbox",
        astra_viewmodel::FolderRole::Drafts => "drafts",
        astra_viewmodel::FolderRole::Sent => "sent",
        astra_viewmodel::FolderRole::Archive => "archive",
        astra_viewmodel::FolderRole::Junk => "junk",
        astra_viewmodel::FolderRole::Trash => "trash",
        astra_viewmodel::FolderRole::Other => "other",
    }
    .to_owned()
}

async fn list_messages(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: ListMessagesArgs,
) -> Result<Value, ToolFailure> {
    require_allowed(gate, &args.account)?;
    let page = backend
        .folder_page(
            &args.account,
            args.folder.as_deref(),
            args.unread_only,
            args.offset,
            args.limit.unwrap_or(DEFAULT_PAGE),
        )
        .await;
    Ok(page_out(&page))
}

async fn search_messages(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: SearchArgs,
) -> Result<Value, ToolFailure> {
    if args.account.is_none() && args.folder.is_some() {
        return Err(ToolFailure::BadArgs(
            "`folder` narrows within one account, so it requires `account` too".to_owned(),
        ));
    }
    if let Some(account) = args.account.as_deref() {
        require_allowed(gate, account)?;
        let page = backend
            .search(
                &args.query,
                Some(account),
                args.folder.as_deref(),
                args.offset,
                args.limit.unwrap_or(DEFAULT_PAGE),
            )
            .await;
        return Ok(page_out(&page));
    }
    // An unscoped search runs over exactly the exposed accounts, under
    // the no-account scope rules: every folder but each account's own
    // Trash. An empty exposure answers an empty page without reaching
    // the mail layer at all.
    let allowed = gate.allowed();
    if allowed.is_empty() {
        return Ok(page_out(&MessagePage {
            rows: Vec::new(),
            total: 0,
            offset: args.offset,
            windowed: false,
        }));
    }
    let page = backend
        .search_set(
            &args.query,
            &allowed,
            args.offset,
            args.limit.unwrap_or(DEFAULT_PAGE),
        )
        .await;
    Ok(page_out(&page))
}

/// One page of rows as the listing and search tools answer it: no bodies,
/// the window flag tellable.
fn page_out(page: &MessagePage) -> Value {
    json!({
        "rows": page.rows.iter().map(row_out).collect::<Vec<_>>(),
        "total": page.total,
        "offset": page.offset,
        "windowed": page.windowed,
    })
}

/// One row of a listing or a search: identity, subject, sender, date, and
/// flags. No body crosses here.
fn row_out(row: &QueryRow) -> Value {
    json!({
        "account": row.account.as_str(),
        "key": row.key.as_str(),
        "subject": row.subject,
        "sender": row.sender,
        "date": row.date,
        "unread": row.unread,
        "flagged": row.flagged,
        "has_attachment": row.has_attachment,
    })
}

async fn get_message(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: MessageArgs,
) -> Result<Value, ToolFailure> {
    require_allowed(gate, &args.account)?;
    let detail = backend
        .message(&args.account, &args.key)
        .await
        .ok_or_else(|| ToolFailure::Refused("no such message in that account".to_owned()))?;
    if detail.load_error {
        return Err(ToolFailure::Refused(
            "the message body could not be fetched: the account may be offline".to_owned(),
        ));
    }
    Ok(message_out(&args.account, &args.key, &detail))
}

/// One message in full: the one place a body crosses this boundary, and
/// the one place the fence goes on.
fn message_out(account: &str, key: &str, detail: &MessageDetail) -> Value {
    json!({
        "account": account,
        "key": key,
        "subject": detail.subject,
        "from": detail.from,
        "to": detail.to,
        "cc": detail.cc,
        "bcc": detail.bcc,
        "date": detail.date,
        "unread": detail.unread,
        "flagged": detail.flagged,
        "attachment_names": detail.attachment_names,
        "body": fence::fence(&detail.body_text),
    })
}

async fn mark_read(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: MarkReadArgs,
) -> Result<Value, ToolFailure> {
    require_allowed(gate, &args.account)?;
    backend
        .mark_read(&args.account, &args.key, args.read)
        .await
        .map_err(write_failure)?;
    let outcome = if args.read {
        "marked read"
    } else {
        "marked unread"
    };
    Ok(done(&args.account, Some(&args.key), outcome))
}

async fn set_flagged(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: SetFlaggedArgs,
) -> Result<Value, ToolFailure> {
    require_allowed(gate, &args.account)?;
    backend
        .set_flagged(&args.account, &args.key, args.flagged)
        .await
        .map_err(write_failure)?;
    Ok(done(
        &args.account,
        Some(&args.key),
        if args.flagged { "flagged" } else { "unflagged" },
    ))
}

async fn create_draft(
    backend: &dyn MailBackend,
    gate: &Gate,
    args: DraftArgs,
) -> Result<Value, ToolFailure> {
    // The account the draft composes from: the named one, else the app's
    // own default among the exposed (its first configured account the gate
    // allows). An agent with no exposed account drafts from nowhere.
    let account = match args.account.as_deref() {
        Some(account) => account.to_owned(),
        None => backend
            .accounts()
            .await
            .iter()
            .find(|account| gate.allows(account.id.as_str()))
            .map(|account| account.id.as_str().to_owned())
            .ok_or_else(|| {
                ToolFailure::Refused(
                    "no account is available to assistants; the user chooses which to expose \
                     in the app's advanced settings"
                        .to_owned(),
                )
            })?,
    };
    require_allowed(gate, &account)?;
    if let Some(reply) = &args.reply_to {
        require_allowed(gate, &reply.account)?;
    }
    let draft = AgentDraft {
        account: Some(account.clone()),
        to: join(&args.to),
        cc: join(&args.cc),
        bcc: join(&args.bcc),
        subject: args.subject,
        body_text: args.body_text,
        reply_to_account: args.reply_to.as_ref().map(|reply| reply.account.clone()),
        reply_to_key: args.reply_to.map(|reply| reply.key),
    };
    backend.open_composer(draft).await.map_err(draft_failure)?;
    Ok(done(
        &account,
        None,
        "opened a draft in the user's composer; they have not sent it yet",
    ))
}

/// Renders a composer-open failure as the refusal the model can relay.
fn draft_failure(error: OpenDraftError) -> ToolFailure {
    ToolFailure::Refused(match error {
        OpenDraftError::NoAccount => "no account is configured to compose a draft from".to_owned(),
    })
}

/// Joins recipient addresses into the comma-separated field shape the
/// composer takes.
fn join(addresses: &[String]) -> String {
    addresses
        .iter()
        .map(|address| address.trim())
        .filter(|address| !address.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "../bridge_tests.rs"]
mod tests;
