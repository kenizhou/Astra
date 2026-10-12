// SPDX-License-Identifier: GPL-3.0-only
//! The port this crate reaches the running app through.
//!
//! # Why a trait rather than `App<P>` directly
//!
//! The app is generic over its provider (`App<P: Provider>`), so taking it
//! directly would make the server, the bridge, and every tool generic too:
//! viral parameters through a crate that has no opinion about providers at
//! all. The port is declared here, implemented for every `App<P>` in this
//! crate (where the astra-app dependency is spent), and faked in a page of
//! code by the tests, so the wire format, the gate, and the ledger are
//! tested without an engine or a provider.
//!
//! Every method maps 1:1 onto a `query_*` read or a write method the
//! intents dispatch to. The port deliberately adds no logic: ordering,
//! search scope, and write semantics all live in astra-app, so an agent
//! and a person are never shown two different mailboxes.

use astra_app::{App, MailWriteError, MessageDetail, MessagePage, OpenDraftError, QueryAccount};
use astra_viewmodel::FolderRow;
use engine_api::{AccountId, Provider};

/// A draft to open, unsent, in the app's own composer.
///
/// The recipient fields are comma-joined rather than lists, matching the
/// shape the compose-request surface takes: one representation of "a
/// recipient field", not two.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDraft {
    /// The account to compose from, or `None` to let the caller resolve one.
    pub account: Option<String>,
    /// The `To` field, comma-joined.
    pub to: String,
    /// The `Cc` field, comma-joined.
    pub cc: String,
    /// The `Bcc` field, comma-joined. The compose-request snapshot carries
    /// no Bcc, so the composer opens without it (the known gap the design
    /// records); the field stays on the port so the gap is visible at the
    /// seam rather than widened silently later.
    pub bcc: String,
    /// The subject.
    pub subject: String,
    /// The body, as plain text.
    pub body_text: String,
    /// The account of the message being replied to, if this is a reply.
    pub reply_to_account: Option<String>,
    /// The provider key of the message being replied to, if this is a reply.
    pub reply_to_key: Option<String>,
}

/// The running app, as this crate needs it. Every method is
/// infallible-or-typed: nothing here returns a rendered message, because
/// user-facing strings belong in a client's catalog, not on a wire an
/// assistant reads.
#[async_trait::async_trait]
pub trait MailBackend: Send + Sync + 'static {
    /// Every configured account, as `(id, address)` rows.
    async fn accounts(&self) -> Vec<QueryAccount>;

    /// One account's folders, in canonical sidebar order.
    async fn folders(&self, account: &str) -> Vec<FolderRow>;

    /// One page of a folder's messages, newest first.
    async fn folder_page(
        &self,
        account: &str,
        folder: Option<&str>,
        unread_only: bool,
        offset: usize,
        limit: usize,
    ) -> MessagePage;

    /// One page of search hits, newest first, over one named account.
    async fn search(
        &self,
        query: &str,
        account: Option<&str>,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> MessagePage;

    /// One page of search hits over exactly `accounts`, every folder but
    /// each account's Trash: the set form the gate needs, so a caller that
    /// may see only some accounts is never answered a superset.
    async fn search_set(
        &self,
        query: &str,
        accounts: &[String],
        offset: usize,
        limit: usize,
    ) -> MessagePage;

    /// One message in full, **without marking it read**.
    async fn message(&self, account: &str, key: &str) -> Option<MessageDetail>;

    /// Marks a message read or unread.
    async fn mark_read(&self, account: &str, key: &str, read: bool) -> Result<(), MailWriteError>;

    /// Flags or unflags a message.
    async fn set_flagged(
        &self,
        account: &str,
        key: &str,
        flagged: bool,
    ) -> Result<(), MailWriteError>;

    /// Moves a message to its account's Archive folder.
    async fn archive(&self, account: &str, key: &str) -> Result<(), MailWriteError>;

    /// Moves a message to its account's Trash folder (recoverable).
    async fn trash(&self, account: &str, key: &str) -> Result<(), MailWriteError>;

    /// Opens `draft` in the app's own composer, unsent.
    ///
    /// # Errors
    ///
    /// [`OpenDraftError::NoAccount`] when no account is configured to
    /// compose from.
    async fn open_composer(&self, draft: AgentDraft) -> Result<(), OpenDraftError>;
}

/// Parses an account id, or `None` if it is malformed.
fn account_id(account: &str) -> Option<AccountId> {
    AccountId::try_from(account).ok()
}

/// Builds the typed message reference the app's writes take, or the
/// unknown-account error a malformed half earns. The pair travels together
/// all the way from the MCP boundary, mirroring `MessageRef::from_parts`:
/// a provider key is unique only within an account.
fn message_row(account: &str, key: &str) -> Result<astra_app::RowRef, MailWriteError> {
    astra_app::MessageRef::from_parts(account, key.to_owned())
        .map(astra_app::RowRef::Message)
        .ok_or(MailWriteError::UnknownAccount)
}

#[async_trait::async_trait]
impl<P: Provider + 'static> MailBackend for App<P> {
    async fn accounts(&self) -> Vec<QueryAccount> {
        self.query_accounts().await
    }

    async fn folders(&self, account: &str) -> Vec<FolderRow> {
        match account_id(account) {
            Some(id) => self.query_folders(&id).await,
            None => Vec::new(),
        }
    }

    async fn folder_page(
        &self,
        account: &str,
        folder: Option<&str>,
        unread_only: bool,
        offset: usize,
        limit: usize,
    ) -> MessagePage {
        let Some(id) = account_id(account) else {
            return MessagePage::default();
        };
        self.query_messages(&id, folder, unread_only, offset, limit)
            .await
    }

    async fn search(
        &self,
        query: &str,
        account: Option<&str>,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> MessagePage {
        // A malformed id fails CLOSED, like every sibling: folding it to
        // `None` would run the unscoped search and leak accounts the gate
        // never exposed.
        let Some(id) = account.and_then(account_id) else {
            return MessagePage::default();
        };
        self.query_search(query, Some(&id), folder, offset, limit)
            .await
    }

    async fn search_set(
        &self,
        query: &str,
        accounts: &[String],
        offset: usize,
        limit: usize,
    ) -> MessagePage {
        let ids: Vec<AccountId> = accounts.iter().filter_map(|id| account_id(id)).collect();
        self.query_search_except_trash_over(query, &ids, offset, limit)
            .await
    }

    async fn message(&self, account: &str, key: &str) -> Option<MessageDetail> {
        let reference = astra_app::MessageRef::from_parts(account, key.to_owned())?;
        self.query_message(&reference).await
    }

    async fn mark_read(&self, account: &str, key: &str, read: bool) -> Result<(), MailWriteError> {
        let row = message_row(account, key)?;
        self.mark_read(row, read).await
    }

    async fn set_flagged(
        &self,
        account: &str,
        key: &str,
        flagged: bool,
    ) -> Result<(), MailWriteError> {
        let row = message_row(account, key)?;
        self.set_flagged(row, flagged).await
    }

    async fn archive(&self, account: &str, key: &str) -> Result<(), MailWriteError> {
        let row = message_row(account, key)?;
        self.archive_message(row).await
    }

    async fn trash(&self, account: &str, key: &str) -> Result<(), MailWriteError> {
        let row = message_row(account, key)?;
        self.move_to_trash(row).await
    }

    async fn open_composer(&self, draft: AgentDraft) -> Result<(), OpenDraftError> {
        // A malformed from-id refuses rather than folding to `None`, which
        // would silently compose from whatever account the app happens to
        // hold first.
        let from = match draft.account.as_deref() {
            None => None,
            Some(id) => Some(account_id(id).ok_or(OpenDraftError::NoAccount)?),
        };
        let reply_to = match (draft.reply_to_account.as_deref(), draft.reply_to_key) {
            (Some(account), Some(key)) => astra_app::MessageRef::from_parts(account, key),
            _ => None,
        };
        let to: Vec<String> = split_addresses(&draft.to);
        let cc: Vec<String> = split_addresses(&draft.cc);
        self.compose_agent_draft(
            from.as_ref(),
            &to,
            &cc,
            &draft.subject,
            &draft.body_text,
            reply_to.as_ref(),
        )
        .await
    }
}

/// One comma-joined recipient field back into its addresses, blanks and
/// surrounding space dropped.
fn split_addresses(joined: &str) -> Vec<String> {
    joined
        .split(',')
        .map(|address| address.trim().to_owned())
        .filter(|address| !address.is_empty())
        .collect()
}
