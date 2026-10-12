// SPDX-License-Identifier: GPL-3.0-only
//! The registered accounts' dial table: each stored account's parsed
//! config and connection pool, keyed by id. The boot restores it from the
//! two stores it lives in, the folder binder reads it, the account-add
//! extends it, and the account store the settings seam drives reads and
//! edits it.
//!
//! The two-store discipline is the boot's: the OS secure store holds each
//! account's config TOML (it is a credential), and a plain `accounts.json`
//! index in the data dir holds the ids, because no platform store can
//! enumerate its entries. The index names what to read; the keyring
//! answers what it holds.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use engine_api::AccountId;

/// The account index's file name, under the data directory.
const ACCOUNT_INDEX_FILE: &str = "accounts.json";

/// The index's on-disk shape: the ids whose configs the keyring holds.
#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountIndex {
    /// The stored accounts' ids, in the order they were added.
    #[serde(default)]
    accounts: Vec<String>,
}

/// One registered account's dial table entry: its parsed config and its connection
/// pool, the two things every later bind and re-dial needs.
struct RegisteredDial {
    /// The account's stored config (endpoints, credentials, accepted certificates).
    config: astra_account::AccountConfig,
    /// The account's connections: the engine account every folder provider of it is
    /// bound through, connected on first use after a restart.
    connections: Arc<astra_account::ImapConnections>,
}

/// Every registered account's dial table, and the index file that names them: what
/// the boot restores, the folder-binder reads, the account-add extends, and the
/// settings seam edits.
///
/// Shared behind an `Arc` between the managed state and the app's connector, so a
/// folder bound later draws on the same pool the account's dial did.
pub(crate) struct AccountDials {
    /// The index file, rewritten whole on every registration.
    index_path: PathBuf,
    /// The registered accounts, keyed by id so the order is stable.
    entries: Mutex<BTreeMap<String, RegisteredDial>>,
}

impl core::fmt::Debug for AccountDials {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The configs carry credentials; the count is the whole safe description.
        f.debug_struct("AccountDials")
            .field("accounts", &self.ids().len())
            .finish_non_exhaustive()
    }
}

impl AccountDials {
    /// An empty table whose index lives in `data_dir`.
    #[cfg(test)]
    pub(crate) fn empty(data_dir: &std::path::Path) -> Self {
        Self {
            index_path: data_dir.join(ACCOUNT_INDEX_FILE),
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Reads the index in `data_dir` and loads each named account's config from the
    /// keyring. An id whose entry is gone is skipped with a warning rather than
    /// failing the boot: the index is what the boot trusts, and one missing secret
    /// must not take the other accounts down.
    ///
    /// # Errors
    ///
    /// Returns the index read's own error, or the wrapping error for an index
    /// that is not readable JSON or an account whose config cannot be read.
    pub(crate) fn load(data_dir: &std::path::Path) -> Result<Self, std::io::Error> {
        let index_path = data_dir.join(ACCOUNT_INDEX_FILE);
        let index: AccountIndex = match std::fs::read_to_string(&index_path) {
            Ok(text) => serde_json::from_str(&text).map_err(|err| {
                std::io::Error::other(format!("the account index could not be read: {err}"))
            })?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => AccountIndex::default(),
            Err(err) => return Err(err),
        };
        let mut entries = BTreeMap::new();
        for id in index.accounts {
            let Some(config_toml) = crate::KeyringCredentialStore::load(&id).map_err(|err| {
                std::io::Error::other(format!("an account could not be read: {err}"))
            })?
            else {
                log::warn!(
                    "boot: an account named by the index has no stored credential; skipping it"
                );
                continue;
            };
            let config = astra_account::load_str(&config_toml).map_err(|err| {
                std::io::Error::other(format!("an account config could not be read: {err}"))
            })?;
            entries.insert(
                id,
                RegisteredDial {
                    config,
                    connections: astra_account::ImapConnections::new(),
                },
            );
        }
        Ok(Self {
            index_path,
            entries: Mutex::new(entries),
        })
    }

    /// Every registered account's id, in order.
    pub(crate) fn ids(&self) -> Vec<String> {
        self.entries
            .lock()
            .expect("dial-table mutex poisoned")
            .keys()
            .cloned()
            .collect()
    }

    /// The ids the index file lists, read back from disk: what a rolled-back add
    /// must have left untouched.
    #[cfg(test)]
    pub(crate) fn indexed_ids(&self) -> Vec<String> {
        std::fs::read_to_string(&self.index_path)
            .ok()
            .and_then(|text| serde_json::from_str::<AccountIndex>(&text).ok())
            .map(|index| index.accounts)
            .unwrap_or_default()
    }

    /// One account's config and connections, for a folder bind or a settings
    /// read; `None` for an id that is not registered.
    pub(crate) fn of(
        &self,
        id: &str,
    ) -> Option<(
        astra_account::AccountConfig,
        Arc<astra_account::ImapConnections>,
    )> {
        self.entries
            .lock()
            .expect("dial-table mutex poisoned")
            .get(id)
            .map(|entry| (entry.config.clone(), Arc::clone(&entry.connections)))
    }

    /// Registers `id` with its config and pool, rewriting the whole index: a
    /// half-written list of ids is exactly the bug the whole-file write avoids.
    ///
    /// # Errors
    ///
    /// Returns the index write's own error; nothing is registered when it
    /// fails.
    pub(crate) fn register(
        &self,
        id: &AccountId,
        config: astra_account::AccountConfig,
        connections: Arc<astra_account::ImapConnections>,
    ) -> Result<(), std::io::Error> {
        let mut entries = self.entries.lock().expect("dial-table mutex poisoned");
        entries.insert(
            id.as_str().to_owned(),
            RegisteredDial {
                config,
                connections,
            },
        );
        rewrite_index(&entries, &self.index_path)
    }

    /// Replaces `id`'s stored config, keeping its connection pool: the dav
    /// merge's landing, whose id set cannot change.
    pub(crate) fn replace_config(&self, id: &str, config: astra_account::AccountConfig) {
        self.entries
            .lock()
            .expect("dial-table mutex poisoned")
            .entry(id.to_owned())
            .and_modify(|entry| entry.config = config);
    }

    /// Drops `id` from the table, rewriting the whole index, and answers
    /// whether an entry went.
    ///
    /// # Errors
    ///
    /// Returns the index write's own error; the entry is put back when it
    /// fails, so the table and the file the next boot reads stay one thing.
    pub(crate) fn remove(&self, id: &str) -> Result<bool, std::io::Error> {
        let mut entries = self.entries.lock().expect("dial-table mutex poisoned");
        let Some(entry) = entries.remove(id) else {
            return Ok(false);
        };
        if let Err(err) = rewrite_index(&entries, &self.index_path) {
            entries.insert(id.to_owned(), entry);
            return Err(err);
        }
        Ok(true)
    }
}

/// Writes the whole index for `entries`: every id the table holds, one
/// file, one write.
///
/// # Errors
///
/// Returns the encoding or the write's own error; the caller's entries are
/// unchanged when it fails.
fn rewrite_index(
    entries: &BTreeMap<String, RegisteredDial>,
    index_path: &std::path::Path,
) -> Result<(), std::io::Error> {
    let index = AccountIndex {
        accounts: entries.keys().cloned().collect(),
    };
    let text = serde_json::to_string_pretty(&index).map_err(|err| {
        std::io::Error::other(format!("the account index could not be written: {err}"))
    })?;
    std::fs::write(index_path, text)
}
