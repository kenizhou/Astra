// SPDX-License-Identifier: GPL-3.0-only
//! The boot sequence: assemble the host's pieces (the data directory, the runtime,
//! the keyring store, the observer bridge, the stored accounts) and hand them to
//! [`astra_app::boot`], which owns the ordering the cached-first-paint contract rests
//! on. Ported from mail-calendar's bindings boot, minus the registry: the accounts
//! arrive here already dialled, because `astra-app` takes them host-connected.
//!
//! The stored-account restore is two stores by design: the OS secure store holds each
//! account's config TOML (it is a credential), and a plain `accounts.json` index in
//! the data dir holds the ids, because no platform store can enumerate its entries.
//! The index names what to read; the keyring answers what it holds.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use engine_api::{AccountId, EmailAddress};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use astra_app::{MailboxConnector, SpawnedTask};

use crate::{
    build_runtime,
    observer::{DebouncedObserver, ObserverBridge},
    AppPort, AppState, HostAccount, HostProvider, KeyringCredentialStore, TauriEventSink,
};

/// The engine store's file name, under the data directory.
const ENGINE_STORE_FILE: &str = "astra.sqlite";

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
/// the boot restores, the folder-binder reads, and the account-add extends.
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
    pub(crate) fn empty(data_dir: &Path) -> Self {
        Self {
            index_path: data_dir.join(ACCOUNT_INDEX_FILE),
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Reads the index in `data_dir` and loads each named account's config from the
    /// keyring. An id whose entry is gone is skipped with a warning rather than
    /// failing the boot: the index is what the boot trusts, and one missing secret
    /// must not take the other accounts down.
    pub(crate) fn load(data_dir: &Path) -> Result<Self, std::io::Error> {
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
            let Some(config_toml) = KeyringCredentialStore::load(&id).map_err(|err| {
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

    /// One account's config and connections, for a folder bind; `None` for an id
    /// that is not registered.
    fn of(
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
        let index = AccountIndex {
            accounts: entries.keys().cloned().collect(),
        };
        let text = serde_json::to_string_pretty(&index).map_err(|err| {
            std::io::Error::other(format!("the account index could not be written: {err}"))
        })?;
        std::fs::write(&self.index_path, text)
    }
}

/// Binds a folder of a registered account on demand: the host's implementation of
/// the app's [`MailboxConnector`] seam, over the dial table.
struct DialConnector {
    /// The registered accounts.
    dials: Arc<AccountDials>,
}

#[async_trait]
impl MailboxConnector<HostProvider> for DialConnector {
    async fn connect_folder(&self, account: &AccountId, mailbox_key: &str) -> Option<HostProvider> {
        let (config, connections) = self.dials.of(account.as_str())?;
        astra_account::connect_imap_mailbox(&connections, &config, mailbox_key)
            .await
            .ok()
    }
}

/// Dials every registered account, connected config to bound providers. A dial that
/// fails is skipped with a warning: one stale password must not block the others,
/// and the app opens with whatever connected.
async fn dial_stored(dials: &AccountDials) -> Vec<HostAccount> {
    let mut accounts = Vec::new();
    for id in dials.ids() {
        let Some((config, connections)) = dials.of(&id) else {
            continue;
        };
        let account_id = match config.account_id() {
            Ok(account_id) => account_id,
            Err(err) => {
                log::warn!("boot: an account could not derive its id and was skipped: {err}");
                continue;
            }
        };
        let email = config
            .imap
            .as_ref()
            .map(|imap| imap.username.clone())
            .unwrap_or_default();
        match astra_account::connect_mail_providers(&connections, &config, &account_id).await {
            Ok(providers) => accounts.push(HostAccount {
                id: account_id,
                providers,
                identity: EmailAddress::new(email),
            }),
            Err(err) => {
                log::warn!("boot: an account could not be connected and was skipped: {err}");
            }
        }
    }
    accounts
}

/// The `setup` hook: builds the host's pieces, boots the app core, and registers the
/// managed state the command surface reads. The dials run to completion here, on the
/// runtime's driver thread, because `astra-app` takes its accounts host-connected; a
/// first run has none and boots instantly.
///
/// # Errors
///
/// Fails when the data directory cannot be made, the runtime cannot start, the
/// stored accounts cannot be read, or the engine cannot open its store: nothing the
/// shell could show a window over.
pub(crate) fn setup_host(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = handle
        .path()
        .app_data_dir()
        .map_err(|err| format!("the app data directory could not be resolved: {err}"))?;
    std::fs::create_dir_all(&data_dir)?;

    let runtime = build_runtime()?;
    let keyring = KeyringCredentialStore::new();
    let observer: Arc<dyn astra_app::AppObserver> = Arc::new(DebouncedObserver::new(
        ObserverBridge::new(Box::new(TauriEventSink::new(handle))),
    ));
    let connections = Arc::new(AccountDials::load(&data_dir)?);

    let accounts = runtime.block_on(|| dial_stored(&connections));
    let engine_path = data_dir.join(ENGINE_STORE_FILE);
    let spawn = {
        let spawner = runtime.handle().clone();
        move |task: SpawnedTask| {
            spawner.spawn(task);
        }
    };
    let core = runtime.block_on(|| {
        astra_app::boot(
            &engine_path,
            accounts,
            Some(Box::new(DialConnector {
                dials: Arc::clone(&connections),
            })),
            observer,
            spawn,
        )
    })?;

    log::info!(
        "boot: the app core is up over {} account(s)",
        connections.ids().len(),
    );
    app.manage(AppState {
        app: core as Arc<dyn AppPort>,
        runtime,
        connections,
        credential_store: Arc::new(keyring),
    });
    Ok(())
}
