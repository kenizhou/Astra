// SPDX-License-Identifier: GPL-3.0-only
//! The listing and search queries: the read side of "what is in my inbox?"
//! and "find the one about invoices". See the module root for the door's own
//! rules; both reads here answer from the store directly, never through the
//! caches the pane projects from.

use engine_api::{AccountId, MailListRow, Mailbox, MailboxRole, Provider};

use super::{MessagePage, MAX_PAGE};
use crate::{
    list_rows::{format_instant, newest_first, tiebreak},
    search::{role_key, FETCH_LIMIT},
    window::MIN_LOAD_WINDOW,
    App,
};

/// The deepest store read a query will drive, in messages. Paging here is a
/// window, not a cursor: `offset + limit` decides how far back the read
/// reaches, and this caps it, so a client asking for an enormous offset
/// cannot turn a listing into a full-mailbox read.
const MAX_QUERY_WINDOW: usize = 5_000;

impl<P: Provider> App<P> {
    /// One page of `folder`'s messages on `account`, newest first. `folder`
    /// is a provider key; `None` lists the account's whole mailbox.
    /// `unread_only` narrows to unread mail, and is reflected in
    /// [`MessagePage::total`] so a caller can say "12 unread" honestly.
    ///
    /// The page is cut from the account's newest-`N` slice, read straight
    /// from the store over the same floor the pane's own load reads use. A
    /// folder whose mail is all older than that slice is not reachable by
    /// raising `offset`; the bound on the read is what keeps one query
    /// cheap, and the same trade-off the folder view makes.
    pub async fn query_messages(
        &self,
        account: &AccountId,
        folder: Option<&str>,
        unread_only: bool,
        offset: usize,
        limit: usize,
    ) -> MessagePage {
        let limit = limit.clamp(1, MAX_PAGE);
        let window = offset
            .saturating_add(limit)
            .clamp(MIN_LOAD_WINDOW, MAX_QUERY_WINDOW);
        let base = self
            .engine
            .mail_window(std::slice::from_ref(account), window)
            .await
            .unwrap_or_default();
        let mut kept: Vec<&MailListRow> = base
            .iter()
            .filter(|row| in_folder(row, folder) && (!unread_only || row.mail.flags.is_unread()))
            .collect();
        kept.sort_by(|a, b| newest_first(a, b).then_with(|| tiebreak(a, b)));
        let total = kept.len();
        let rows = kept
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(query_row)
            .collect();
        MessagePage {
            rows,
            total,
            offset,
        }
    }

    /// Searches mail and returns one page of hits, **newest first**, ordered
    /// by date rather than relevance (the same rule the product's own search
    /// answers under; the engine's ranking still decides which hits arrive).
    ///
    /// The scope vocabulary is the search contract's own, resolved against
    /// the query's two optional narrowings rather than the live selection:
    /// no account named means every account and every folder except Trash
    /// (the product default), an account named with no folder means that
    /// account's whole mailbox (Trash included), and a folder is honoured
    /// only beside its account, the shape every tool schema pins. Nothing
    /// about the user's own search is read or written: an assistant
    /// searching cannot narrow, widen, or clear what the person is looking
    /// at.
    pub async fn query_search(
        &self,
        query: &str,
        account: Option<&AccountId>,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> MessagePage {
        let limit = limit.clamp(1, MAX_PAGE);
        if query.trim().is_empty() {
            // A query of nothing but whitespace names nobody: the honest
            // answer is no rows, never the whole mailbox an empty engine
            // query would match.
            return MessagePage {
                rows: Vec::new(),
                total: 0,
                offset,
            };
        }
        let handles = match account {
            Some(id) => self
                .account_handle(id)
                .await
                .map_or_else(Vec::new, |handle| vec![handle]),
            None => self.account_handles().await,
        };
        let mut hits: Vec<MailListRow> = Vec::new();
        for handle in handles {
            let tree = self.engine.mailboxes(&handle.id).await.unwrap_or_default();
            let filter = hit_filter(account, folder, &tree);
            let ranked = self
                .engine
                .search_mail(&handle.id, query, FETCH_LIMIT)
                .await
                .map(|results| results.hits)
                .unwrap_or_default();
            let keys: Vec<_> = ranked.iter().map(|hit| hit.key.clone()).collect();
            let resolved = self
                .engine
                .mail_by_keys(&handle.id, &keys)
                .await
                .unwrap_or_default();
            hits.extend(resolved.into_iter().filter(|row| filter.keeps(row)));
        }
        hits.sort_by(|a, b| newest_first(a, b).then_with(|| tiebreak(a, b)));
        let total = hits.len();
        let rows = hits
            .iter()
            .skip(offset)
            .take(limit)
            .map(query_row)
            .collect();
        MessagePage {
            rows,
            total,
            offset,
        }
    }
}

/// Which of one account's matched rows the search scope keeps.
enum HitFilter {
    /// Everything except the account's Trash; `None` when it has no trash
    /// folder, in which case nothing is excluded.
    ExceptTrash(Option<String>),
    /// The account's whole mailbox, Trash included.
    Everything,
    /// Only the messages filed in this folder key.
    Only(String),
}

impl HitFilter {
    /// Whether this filter keeps `row`.
    fn keeps(&self, row: &MailListRow) -> bool {
        let filed_in = |key: &str| row.mailboxes.iter().any(|mb| mb.as_str() == key);
        match self {
            Self::ExceptTrash(trash) => trash.as_deref().is_none_or(|key| !filed_in(key)),
            Self::Everything => true,
            Self::Only(key) => filed_in(key),
        }
    }
}

/// The filter for one account under the query's narrowings, resolved against
/// that account's own tree.
fn hit_filter(account: Option<&AccountId>, folder: Option<&str>, tree: &[Mailbox]) -> HitFilter {
    match (account, folder) {
        // No account named: every folder except Trash. A folder without its
        // account narrows nothing here; the tool schema pins that a folder
        // always travels with its account.
        (None, _) => HitFilter::ExceptTrash(role_key(tree, &MailboxRole::Trash)),
        (Some(_), None) => HitFilter::Everything,
        (Some(_), Some(key)) => HitFilter::Only(key.to_owned()),
    }
}

/// Whether `row` is filed in `folder` (`None` is the account's whole
/// mailbox). Placement is a separate axis from identity: a provider object
/// can hold several memberships.
fn in_folder(row: &MailListRow, folder: Option<&str>) -> bool {
    folder.is_none_or(|key| row.mailboxes.iter().any(|id| id.as_str() == key))
}

/// One store row as the query's answer row.
fn query_row(row: &MailListRow) -> super::QueryRow {
    super::QueryRow {
        account: row.account.clone(),
        key: row.mail.key.clone(),
        subject: row.mail.subject.clone().unwrap_or_default(),
        sender: sender_line(row),
        date: row.mail.date_utc.map_or_else(String::new, format_instant),
        unread: row.mail.flags.is_unread(),
        flagged: row.mail.flags.flagged(),
        has_attachment: row.mail.has_attachment,
    }
}

/// The sender as `Name <email>`, the bare address, or empty when there is no
/// sender at all.
fn sender_line(row: &MailListRow) -> String {
    let name = row
        .mail
        .from_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let address = row
        .mail
        .from_addr
        .as_deref()
        .filter(|addr| !addr.is_empty());
    match (name, address) {
        (Some(name), Some(address)) if name != address => format!("{name} <{address}>"),
        (_, Some(address)) => address.to_owned(),
        (Some(name), None) => name.to_owned(),
        (None, None) => String::new(),
    }
}
