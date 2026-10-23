// SPDX-License-Identifier: GPL-3.0-only
//! The search service: the query DSL split, the run over the engine's search
//! surface, and the state a search publishes onto the mailbox-list snapshot.
//! The engine owns matching; this owns which accounts and folders answer (the
//! scope, resolved against the live selection), how deep the answer looked
//! (the synced window, in whole months), and the rows the answer shows (the
//! projection in `list_rows`, the same one the folder view draws).

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Instant,
};

use astra_viewmodel::{MailboxListSnapshot, MessageRowView, SearchScope, SearchState, ViewMode};
use engine_api::{AccountId, MailListRow, Mailbox, MailboxRole, Provider, ThreadId};

use crate::{
    list_rows::{self, SearchMessage},
    snapshot::account_folder_rows,
    Account, App, Scope,
};

/// How many candidates one account's engine query may return. The engine ranks
/// each account's hits by relevance and caps them; this reads that as a
/// candidate set to re-order by date, so the cap bounds the store read, not
/// the answer's order.
pub(crate) const FETCH_LIMIT: usize = 500;

/// How many rows one search shows. Results stop here with no show-more,
/// matching the ported contract: a query matching thousands shows the newest
/// rows and states the horizon beside them rather than paging the rest.
const RESULT_LIMIT: usize = 100;

/// The live search session: what the field holds and how far the search
/// reaches, held together because a rebuild needs both to mean one search.
/// Session state, never persisted: the scope is a filter on one search, not a
/// preference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SearchSession {
    /// The query the field holds, or `None` when the list is not a search.
    pub(crate) query: Option<String>,
    /// The scope the live query runs under.
    pub(crate) scope: SearchScope,
}

/// What one search found: the rows to show, and how far back it looked.
struct SearchOutcome {
    /// The projected result rows, newest first.
    rows: Vec<MessageRowView>,
    /// The searched horizon in whole months.
    depth_months: u32,
}

impl<P: Provider> App<P> {
    /// Sets the live query, or clears the search. Clearing drops the scope
    /// with it, in the same write: a narrowing the user can no longer see is
    /// one they will not think of, so the next search opens across everything
    /// rather than silently inheriting how the last one was narrowed.
    pub(crate) fn set_search(&self, query: Option<String>) {
        let mut session = self.search.lock().expect("search mutex poisoned");
        if query.is_none() {
            session.scope = SearchScope::default();
        }
        session.query = query;
    }

    /// Narrows the live search to `scope`, returning whether one was live to
    /// narrow. Without a query there is nothing to re-run and nothing stored:
    /// the scope is a property of a search, not a preference.
    pub(crate) fn narrow_search(&self, scope: SearchScope) -> bool {
        let mut session = self.search.lock().expect("search mutex poisoned");
        if session.query.is_some() {
            session.scope = scope;
            true
        } else {
            false
        }
    }

    /// The live session, read whole: the query and the scope are one search,
    /// so a rebuild takes them from one lock rather than pairing two reads
    /// that a keystroke could have split.
    pub(crate) fn search_session(&self) -> SearchSession {
        self.search.lock().expect("search mutex poisoned").clone()
    }

    /// The mailbox-list snapshot while a query is live: the same pane the
    /// folder view draws, with the search state beside it and the folder rows
    /// stood down. The renderer picks the search rows while they are the list
    /// on screen; leaving search publishes the folder rows back.
    pub(crate) async fn search_snapshot(
        &self,
        query: &str,
        scope: SearchScope,
        live: &Scope,
        mode: ViewMode,
        handles: &[Arc<Account<P>>],
        limit: usize,
    ) -> MailboxListSnapshot {
        let ids: Vec<AccountId> = handles.iter().map(|account| account.id.clone()).collect();
        let trees = self.load_rows(&ids).await;
        let account_folders = account_folder_rows(handles, &trees, limit);
        let started = Instant::now();
        let outcome = self
            .run_search(query, scope, live, mode, &ids, &trees)
            .await;
        log::info!(
            "search: {} row(s), {} month(s) deep, in {}ms",
            outcome.rows.len(),
            outcome.depth_months,
            started.elapsed().as_millis()
        );
        MailboxListSnapshot {
            account_folders,
            rows: Vec::new(),
            selection: None,
            visible_limit: u32::try_from(limit).unwrap_or(u32::MAX),
            view_mode: mode,
            search: Some(SearchState {
                query: query.to_owned(),
                scope,
                rows: outcome.rows,
                depth_months: outcome.depth_months,
            }),
        }
    }

    /// Runs the query over every account the scope covers: each account's
    /// engine query in turn, its ranked keys resolved from the store (a search
    /// legitimately matches older mail the windowed cache never held), the
    /// scope's folder filter applied app-side, and the survivors pooled for
    /// the projection to order newest first.
    ///
    /// The `from:`/`to:` filters are passed to the engine inside its own query
    /// string: the engine's search surface accepts them there, so no app-side
    /// narrowing runs.
    async fn run_search(
        &self,
        query: &str,
        scope: SearchScope,
        live: &Scope,
        mode: ViewMode,
        ids: &[AccountId],
        trees: &[Arc<Vec<Mailbox>>],
    ) -> SearchOutcome {
        let (from, to, text) = split_query(query);
        let dsl = engine_query(from.as_deref(), to.as_deref(), &text);
        let covered = searched(scope, live, ids, trees);
        let depth_months = self.depth(covered.iter().map(|(account, _)| *account));
        if dsl.trim().is_empty() {
            // A query of nothing but malformed prefixes names nobody: the
            // honest answer is no rows, never the whole mailbox an empty
            // engine query would match.
            return SearchOutcome {
                rows: Vec::new(),
                depth_months,
            };
        }
        let mut messages: Vec<SearchMessage> = Vec::new();
        for (account, filter) in covered {
            let ranked = self
                .engine
                .search_mail(account, &dsl, FETCH_LIMIT)
                .await
                .map(|results| results.hits)
                .unwrap_or_default();
            let keys: Vec<_> = ranked.iter().map(|hit| hit.key.clone()).collect();
            let resolved = self
                .engine
                .mail_by_keys(account, &keys)
                .await
                .unwrap_or_default();
            messages.extend(
                resolved
                    .into_iter()
                    .filter(|row| filter.keeps(row))
                    .map(|row| SearchMessage {
                        row: Arc::new(row),
                        matched: true,
                    }),
            );
        }
        if mode == ViewMode::Threaded {
            self.complete_search_threads(&mut messages, ids).await;
        }
        SearchOutcome {
            rows: list_rows::search_project(mode, &messages, RESULT_LIMIT),
            depth_months,
        }
    }

    /// Adds the non-matching members of every conversation the hits touch, so
    /// a threaded search row carries its whole conversation rather than the
    /// fragment that matched. Each addition is marked unmatched, which is what
    /// keeps the two rules of a threaded search true: a conversation nobody's
    /// query touched is never listed, and one sorts on its newest match rather
    /// than on a reply the query said nothing about. One indexed read for
    /// every shown thread, never one per thread.
    async fn complete_search_threads(&self, messages: &mut Vec<SearchMessage>, ids: &[AccountId]) {
        let threads: BTreeSet<&str> = messages
            .iter()
            .filter(|message| message.matched)
            .filter_map(|message| message.row.mail.thread_id.as_ref().map(ThreadId::as_str))
            .collect();
        if threads.is_empty() {
            return;
        }
        let present: HashSet<(String, String)> = messages
            .iter()
            .map(|message| {
                (
                    message.row.account.as_str().to_owned(),
                    message.row.mail.key.as_str().to_owned(),
                )
            })
            .collect();
        let extra = self
            .engine
            .mail_on_threads(ids, threads.iter().copied())
            .await
            .unwrap_or_default();
        messages.extend(
            extra
                .into_iter()
                .filter(|row| {
                    !present.contains(&(
                        row.account.as_str().to_owned(),
                        row.mail.key.as_str().to_owned(),
                    ))
                })
                .map(|row| SearchMessage {
                    row: Arc::new(row),
                    matched: false,
                }),
        );
    }

    /// How far back the searched accounts hold mail, in whole months: the
    /// narrowest of their sync depths, since one shallow account makes the
    /// whole answer that old at best. Zero when the scope searched no
    /// account at all: there is no corpus to describe, and an answer that
    /// ran nowhere claims no horizon either.
    fn depth<'a>(&self, searched: impl IntoIterator<Item = &'a AccountId>) -> u32 {
        searched
            .into_iter()
            .map(|account| self.sync_depth_months(account))
            .reduce(u32::min)
            .unwrap_or(0)
    }

    /// One account's synced window, in whole months: the depth the pass
    /// window runs under, read from the same preference.
    fn sync_depth_months(&self, account: &AccountId) -> u32 {
        let pane = self.pane.lock().expect("preferences mutex poisoned");
        u32::from(pane.sync_settings(account).depth_months)
    }
}

/// Splits a raw search query into its `from:` and `to:` prefix filters and the
/// free-text remainder, mirroring the engine's own parser: the operator
/// keyword matches case-insensitively and the value crosses verbatim, an
/// unknown prefix (or a colon inside a quoted span) stays free text, and a
/// prefix with no value is dropped rather than passed on, so the engine is
/// never handed a query it would refuse outright. Repeating a prefix keeps the
/// later value; the split's seam carries one filter per field.
pub(crate) fn split_query(raw: &str) -> (Option<String>, Option<String>, String) {
    let mut split = (None, None, Vec::new());
    for token in tokens(raw) {
        match prefix(&token) {
            Some((Prefix::From, value)) => {
                if !value.is_empty() {
                    split.0 = Some(value);
                }
            }
            Some((Prefix::To, value)) => {
                if !value.is_empty() {
                    split.1 = Some(value);
                }
            }
            None => split.2.push(token),
        }
    }
    (split.0, split.1, split.2.join(" "))
}

/// The prefix operators this split knows: exactly the two the product DSL
/// names. The engine's wider vocabulary (`subject:`, `before:`, and the rest)
/// is the engine's to parse, so those tokens stay in the remainder untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prefix {
    /// `from:`: the sender's address.
    From,
    /// `to:`: a recipient's address.
    To,
}

/// Classifies one token: its prefix and verbatim value when it is a `from:` or
/// `to:` operator, or `None` when it is free text. A colon inside a quoted span
/// does not make an operator (`"a:b"` is a phrase), the engine's own rule.
fn prefix(token: &str) -> Option<(Prefix, String)> {
    let colon = token.find(':')?;
    let head = &token[..colon];
    if head.contains('"') {
        return None;
    }
    let value = unquote(&token[colon + 1..]);
    match head.to_ascii_lowercase().as_str() {
        "from" => Some((Prefix::From, value)),
        "to" => Some((Prefix::To, value)),
        _ => None,
    }
}

/// Splits `raw` into whitespace-separated tokens, honouring double-quoted
/// spans: a quoted value keeps its spaces. The quotes stay on the token, the
/// engine's own rule, so a phrase that reaches the free-text remainder reads
/// back the way it was typed. An unbalanced quote keeps its token open to the
/// end; the engine then refuses the query, as it refuses the raw string.
fn tokens(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in raw.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quote {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Strips one layer of surrounding double quotes, the engine's own rule.
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

/// Rebuilds the query string the engine parses: the extracted filters back as
/// `from:`/`to:` tokens, then the free-text remainder verbatim. The engine
/// accepts the address filters in its own DSL, so they are passed through
/// there rather than narrowed app-side.
fn engine_query(from: Option<&str>, to: Option<&str>, text: &str) -> String {
    let mut query = String::new();
    for (operator, value) in [("from:", from), ("to:", to)] {
        if let Some(value) = value {
            query.push_str(operator);
            query.push_str(&quoted(value));
            query.push(' ');
        }
    }
    query.push_str(text);
    query.trim().to_owned()
}

/// Re-quotes a value that holds whitespace, so it stays one token on its way
/// back to the engine.
fn quoted(value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.to_owned()
    }
}

/// The accounts the scope covers, each paired with the filter its hits must
/// pass. One list, because which accounts a search covers has one answer: the
/// search itself iterates it, and the depth disclosure folds over it.
fn searched<'a>(
    scope: SearchScope,
    live: &Scope,
    ids: &'a [AccountId],
    trees: &'a [Arc<Vec<Mailbox>>],
) -> Vec<(&'a AccountId, HitFilter)> {
    let mut covered = Vec::new();
    for (account, tree) in ids.iter().zip(trees) {
        if let Some(filter) = hit_filter(scope, live, account, tree) {
            covered.push((account, filter));
        }
    }
    covered
}

/// The filter for one account under the search `scope`, resolved against the
/// `live` selection the mailbox list is showing; `None` when the account is
/// outside the scope entirely and should not be searched at all.
///
/// `CurrentFolder` mirrors the list exactly: the selected folder, or (in the
/// unified view) every account's Inbox, an account with none searching
/// nothing. When the list is showing one account's own all-mail view, "the
/// current folder" is that whole mailbox, so the reading converges with
/// `CurrentAccount`'s: both are the account, Trash included.
fn hit_filter(
    scope: SearchScope,
    live: &Scope,
    account: &AccountId,
    tree: &[Mailbox],
) -> Option<HitFilter> {
    match scope {
        SearchScope::AllExceptTrash => {
            Some(HitFilter::ExceptTrash(role_key(tree, &MailboxRole::Trash)))
        }
        SearchScope::CurrentFolder => {
            if matches!(live, Scope::AllInboxes) {
                // The unified view: "this folder" is the set of inboxes on
                // screen, so an account with no Inbox searches nothing.
                role_key(tree, &MailboxRole::Inbox).map(HitFilter::Only)
            } else if names_account(live, account) {
                match live {
                    // One folder is showing: just that folder.
                    Scope::Folder(folder) => Some(HitFilter::Only(folder.key.clone())),
                    // The account's own all-mail view: the whole mailbox, Trash
                    // included, the reading that converges with
                    // CurrentAccount's.
                    _ => Some(HitFilter::Everything),
                }
            } else {
                None
            }
        }
        SearchScope::CurrentAccount => {
            // Whosever mailbox the list is showing, all of it: one account's
            // (a folder of it, or its all-mail view) or every account's (the
            // unified view).
            if matches!(live, Scope::AllInboxes) || names_account(live, account) {
                Some(HitFilter::Everything)
            } else {
                None
            }
        }
    }
}

/// Whether the live selection is showing `account`: one of its folders, or its
/// own all-mail view. The unified view names no single account.
fn names_account(live: &Scope, account: &AccountId) -> bool {
    match live {
        Scope::Folder(folder) => &folder.account == account,
        Scope::Account(selected) => selected == account,
        Scope::AllInboxes => false,
    }
}

/// Which of one account's matched messages the scope keeps.
enum HitFilter {
    /// Everything except the account's Trash; `None` when it has no trash
    /// folder, in which case nothing is excluded.
    ExceptTrash(Option<String>),
    /// Only the messages filed in this folder key.
    Only(String),
    /// Everything the account holds, Trash included.
    Everything,
}

impl HitFilter {
    /// Whether this filter keeps `row`.
    fn keeps(&self, row: &MailListRow) -> bool {
        let filed_in = |key: &str| row.mailboxes.iter().any(|mb| mb.as_str() == key);
        match self {
            Self::ExceptTrash(trash) => trash.as_deref().is_none_or(|key| !filed_in(key)),
            Self::Only(key) => filed_in(key),
            Self::Everything => true,
        }
    }
}

/// The provider key of the folder `tree` tags with `role`, or `None`; resolved
/// from the cached folder trees rather than any per-role key cache.
pub(crate) fn role_key(tree: &[Mailbox], role: &MailboxRole) -> Option<String> {
    tree.iter()
        .find(|mailbox| mailbox.role.as_ref() == Some(role))
        .map(|mailbox| mailbox.id.key().as_str().to_owned())
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

// The scope, grouping, and depth tests split into their own sibling (the same
// mechanism `protocol`'s split test files use) so each stays under the
// 500-line cap.
#[cfg(test)]
#[path = "search_scope_tests.rs"]
mod scope_tests;
