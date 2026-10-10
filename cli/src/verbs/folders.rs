// SPDX-License-Identifier: GPL-3.0-only
//! `folders list`: one account's mailboxes straight from the engine store,
//! the same read the app's folder pane is built from.

use engine_api::{AccountId, Engine, MailboxRole};
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;

/// Lists one account's folders as the store holds them, in the sidebar's own
/// order: role-bearing folders first in their canonical positions, then the
/// rest by the provider's sort hint and name (`astra-viewmodel`'s rule, flat
/// rather than nested because each row carries its parent).
pub(crate) async fn list(engine: &Engine, account: &AccountId) -> Answer {
    let mut mailboxes = engine
        .mailboxes(account)
        .await
        .map_err(runtime("the folders could not be read"))?;
    mailboxes.sort_by_key(sort_key);
    let folders: Vec<Value> = mailboxes
        .iter()
        .map(|mailbox| {
            json!({
                "id": mailbox.id.as_str(),
                "name": mailbox.name,
                "role": mailbox.role.as_ref().map(role_word),
                "parent": mailbox.parent.as_ref().map(engine_api::MailboxId::as_str),
                "unread_count": mailbox.unread_count,
            })
        })
        .collect();
    Ok(json!({ "account": account.as_str(), "folders": folders }))
}

/// The sidebar's sort key: special-use folders lead in their canonical
/// order, ordinary folders follow by sort hint then name.
fn sort_key(mailbox: &engine_api::Mailbox) -> (u8, u8, u32, String) {
    let (group, rank) = match mailbox.role.as_ref().and_then(role_rank) {
        Some(rank) => (0, rank),
        None => (1, 0),
    };
    (group, rank, mailbox.sort_order, mailbox.name.to_lowercase())
}

/// A recognised role's canonical sidebar position.
fn role_rank(role: &MailboxRole) -> Option<u8> {
    Some(match role {
        MailboxRole::Inbox => 0,
        MailboxRole::Drafts => 1,
        MailboxRole::Sent => 2,
        MailboxRole::Archive => 3,
        MailboxRole::Junk => 4,
        MailboxRole::Trash => 5,
        MailboxRole::Flagged => 6,
        MailboxRole::Important => 7,
        MailboxRole::All => 8,
        MailboxRole::Other(_) => return None,
    })
}

/// A role's wire word, the same spelling the engine's serde form uses.
fn role_word(role: &MailboxRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "other".to_owned())
}

/// The engine-error mapping every read verb shares.
pub(crate) fn runtime(context: &'static str) -> impl Fn(engine_api::ApiError) -> (Code, String) {
    move |err| (Code::Runtime, format!("{context}: {err}"))
}
