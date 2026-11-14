// SPDX-License-Identifier: GPL-3.0-only
//! The category overlay: the post-projection fold that sets each row's local
//! categories. The pure projection in `list_rows` stays untouched (it knows
//! nothing of the store); this fold runs fresh on every rebuild after it, so
//! the overlay is never cached (the mail-row cache holds engine rows only).
//!
//! Each row resolves its thread key the way the projection groups: a thread
//! row by its id, a message row by its source row's `thread_key`. The keys
//! group by account, one `assignments_for_threads` query answers each account
//! on the page, and the ids land on the rows.

use std::{collections::HashMap, sync::Arc};

use astra_categories::CategoryId;
use astra_viewmodel::{MessageRowView, RowKey};
use engine_api::{AccountId, MailListRow, Provider};

use crate::{list_rows, App};

impl<P: Provider> App<P> {
    /// Folds the local categories onto the projected `rows`, resolving each
    /// row's thread key against the `source` rows the projection read. An app
    /// with no store installed leaves every row's categories empty.
    pub(crate) fn fold_categories(&self, rows: &mut [MessageRowView], source: &[Arc<MailListRow>]) {
        let Some(store) = self
            .categories
            .lock()
            .expect("categories mutex poisoned")
            .clone()
        else {
            return;
        };
        if rows.is_empty() {
            return;
        }
        // The thread key a message row resolves to: its source row's resolved
        // thread id, or its own provider key when the server never threaded it.
        let thread_of: HashMap<(&str, &str), &str> = source
            .iter()
            .map(|row| {
                (
                    (row.account.as_str(), row.mail.key.as_str()),
                    list_rows::thread_key(row),
                )
            })
            .collect();
        // Each row's (account, thread key), owned so the mutable pass below
        // never borrows the rows it writes.
        let keys: Vec<(AccountId, String)> = rows
            .iter()
            .map(|row| {
                let thread = match &row.key {
                    RowKey::Thread { thread_id, .. } => thread_id.as_str().to_owned(),
                    RowKey::Message { key, .. } => thread_of
                        .get(&(row.account_id.as_str(), key.as_str()))
                        .map_or_else(|| key.as_str().to_owned(), |thread| (*thread).to_owned()),
                };
                (row.account_id.clone(), thread)
            })
            .collect();
        // One overlay query per account on the page.
        let mut by_account: HashMap<AccountId, Vec<String>> = HashMap::new();
        for (account, thread) in &keys {
            by_account
                .entry(account.clone())
                .or_default()
                .push(thread.clone());
        }
        let mut assignments: HashMap<AccountId, HashMap<String, Vec<CategoryId>>> = HashMap::new();
        for (account, threads) in by_account {
            let map = store
                .assignments_for_threads(&account, &threads)
                .unwrap_or_default();
            assignments.insert(account, map);
        }
        for (row, (account, thread)) in rows.iter_mut().zip(keys) {
            row.categories = assignments
                .get(&account)
                .and_then(|map| map.get(&thread))
                .cloned()
                .unwrap_or_default();
        }
    }
}
