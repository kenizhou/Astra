// SPDX-License-Identifier: GPL-3.0-only
//! Stateless mail queries: the read door an agent adapter answers through.
//!
//! # Why these are not Intents
//!
//! Reads go through a different door from writes, and that asymmetry is the
//! load-bearing decision of the whole agent surface. Writes *should* be
//! [`Intent`](crate::Intent)s: an assistant's archive then happens in the
//! user's own list, visibly, by the same mechanism their own click uses.
//! Reads must not be, for two reasons that are easy to miss until a user
//! watches their screen move on its own:
//!
//! 1. Every read-shaped intent ends in a snapshot rebuild, and
//!    [`Intent::Search`](crate::Intent::Search) also rewrites the live query
//!    and scope. An assistant answering "what is in my inbox?" would scroll
//!    and re-scope the window of a person who is reading something else.
//! 2. [`Intent::OpenMessage`](crate::Intent::OpenMessage) marks the message
//!    read on the server. "Read me that email" would silently clear the
//!    unread badge on the user's real mailbox: an irreversible, server-side
//!    side effect of a question.
//!
//! So a query answers a question and changes nothing: no snapshot is
//! republished, no selection moves, no search session is rewritten, and the
//! UI's row caches are not even warmed (a query reads the store directly, so
//! its paging depth cannot perturb what the UI has loaded). The guarantee
//! tests in `query_tests.rs` exist solely to stop a later change collapsing
//! this back into one path, and the first is paired there with its contrast,
//! so the file states why the layer exists rather than only that it works.
//!
//! Reuse, however, is deliberate everywhere it is safe: ordering, folder
//! sorting, and the search scope rules all come from the same definitions
//! the UI uses, so an agent and a person are never shown the same mailbox in
//! two different orders.

use astra_viewmodel::FolderRow;
use engine_api::{AccountId, EmailAddress, Provider, ProviderKey, SystemKeyword};

use crate::{list_rows::format_instant, reading::resolve, reference::MessageRef, App};

mod mail;

/// The largest page any one query returns. The tool schemas in astra-mcp pin
/// the same bound; the two move together.
pub const MAX_PAGE: usize = 50;

/// One configured account as a query names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryAccount {
    /// The account's stable id.
    pub id: AccountId,
    /// The account's own address.
    pub address: String,
}

/// One message row in a listing or a search answer: identity, subject,
/// sender, date, and flags. No body: bodies come only from the one-message
/// read, so a broad listing cannot sweep a hundred bodies into an assistant's
/// context at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRow {
    /// The account that owns the message.
    pub account: AccountId,
    /// The message's provider key, unique only within `account`.
    pub key: ProviderKey,
    /// The subject (empty if none).
    pub subject: String,
    /// The sender, as `Name <email>` or a bare address.
    pub sender: String,
    /// The message's instant as a whole-second RFC 3339 `Z` string (empty if
    /// unknown).
    pub date: String,
    /// Whether the message is unread.
    pub unread: bool,
    /// Whether the message is flagged.
    pub flagged: bool,
    /// Whether the message carries an attachment.
    pub has_attachment: bool,
}

/// One page of message rows, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessagePage {
    /// The rows in this page, newest first.
    pub rows: Vec<QueryRow>,
    /// How many messages match in total, before the page was cut.
    pub total: usize,
    /// The offset this page starts at, echoed back so a caller can page
    /// without bookkeeping.
    pub offset: usize,
}

/// One message in full: headers, flags, and the body as plain text.
///
/// The body is plain text *by construction*: it is the engine's own plain
/// conversion of the raw source, and the HTML half of that conversion is
/// never read here, so a caller over this type structurally cannot emit HTML
/// it was never given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDetail {
    /// The account that owns the message.
    pub account: AccountId,
    /// The message's provider key.
    pub key: ProviderKey,
    /// The subject (empty if none).
    pub subject: String,
    /// The sender, as `Name <email>` or a bare address.
    pub from: String,
    /// The `To` recipients, comma-joined.
    pub to: String,
    /// The `Cc` recipients, comma-joined.
    pub cc: String,
    /// The `Bcc` recipients, comma-joined; populated only on the sender's own
    /// copy.
    pub bcc: String,
    /// The message's instant as a whole-second RFC 3339 `Z` string (empty if
    /// unknown).
    pub date: String,
    /// Whether the message is unread. Reading it through this type does not
    /// change that.
    pub unread: bool,
    /// Whether the message is flagged.
    pub flagged: bool,
    /// The body as plain text, the engine's own conversion. Empty when the
    /// message holds no text the engine could derive.
    pub body_text: String,
    /// The names of the message's attachments. Names only: no bytes cross
    /// this boundary.
    pub attachment_names: Vec<String>,
    /// Whether the body could not be fetched, as distinct from a message that
    /// genuinely has no body.
    pub load_error: bool,
}

impl<P: Provider> App<P> {
    /// Every configured account (id and address). Publishes nothing: the
    /// account list the pane draws is the pane's own rebuild.
    pub async fn query_accounts(&self) -> Vec<QueryAccount> {
        self.account_handles()
            .await
            .into_iter()
            .map(|account| QueryAccount {
                id: account.id.clone(),
                address: account.identity.email.clone(),
            })
            .collect()
    }

    /// One account's folders, in the same canonical order the sidebar shows
    /// (special folders first, the rest by name), read straight from the
    /// store and through the row cache of the pane never. An agent and a
    /// person therefore name the same folder the same way.
    pub async fn query_folders(&self, account: &AccountId) -> Vec<FolderRow> {
        astra_viewmodel::sorted_folder_rows(
            &self.engine.mailboxes(account).await.unwrap_or_default(),
        )
    }

    /// One message in full, **without marking it read**.
    ///
    /// One line different from the open path in `reading`: it resolves the
    /// message and fetches its body directly, never the wrapper that stores
    /// the reading snapshot, signals [`Surface::Reading`](crate::Surface::Reading),
    /// arms the reply draft, and writes `$seen` to the server. That single
    /// difference is why this has a regression test rather than a comment:
    /// the two call sites look alike and the wrong one is silently
    /// destructive.
    pub async fn query_message(&self, message: &MessageRef) -> Option<MessageDetail> {
        let original = resolve(&self.engine, message).await?;
        let handle = self.account_handle(&message.account).await;
        let provider = handle
            .as_deref()
            .and_then(|account| account.providers.first());
        let (body_text, attachment_names, load_error) = match provider {
            Some(provider) => {
                match self
                    .engine
                    .message_body(provider, &message.account, &original)
                    .await
                {
                    Ok(body) => {
                        let names = self
                            .engine
                            .message_attachments(provider, &message.account, &original)
                            .await
                            .unwrap_or_default()
                            .iter()
                            .map(|attachment| attachment.file_name().to_owned())
                            .collect();
                        (body.plain().unwrap_or_default().to_owned(), names, false)
                    }
                    Err(err) => {
                        log::warn!(
                            "query: the body fetch failed for account {}: {err}",
                            message.account.as_str(),
                        );
                        (String::new(), Vec::new(), true)
                    }
                }
            }
            None => (String::new(), Vec::new(), true),
        };
        Some(MessageDetail {
            account: message.account.clone(),
            key: message.key.clone(),
            subject: original.envelope.subject.clone().unwrap_or_default(),
            from: address_list(&original.envelope.from),
            to: address_list(&original.envelope.to),
            cc: address_list(&original.envelope.cc),
            bcc: address_list(&original.envelope.bcc),
            date: original
                .received_at
                .or(original.sent_at)
                .map_or_else(String::new, format_instant),
            unread: !original.has_system_keyword(SystemKeyword::Seen),
            flagged: original.has_system_keyword(SystemKeyword::Flagged),
            body_text,
            attachment_names,
            load_error,
        })
    }
}

/// One address as `Name <email>` or the bare address the header carried.
fn address_line(address: &EmailAddress) -> String {
    match address
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => format!("{name} <{}>", address.email),
        None => address.email.clone(),
    }
}

/// One header's addresses, comma-joined.
fn address_list(addresses: &[EmailAddress]) -> String {
    addresses
        .iter()
        .map(address_line)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "../query_tests.rs"]
mod tests;
