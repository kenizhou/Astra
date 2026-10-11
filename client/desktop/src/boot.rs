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
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use engine_api::{AccountId, EmailAddress};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use astra_app::{MailboxConnector, SpawnedTask};

use crate::{
    agent::{self, chat_model_config, AgentEventSink},
    build_runtime,
    observer::{DebouncedObserver, ObserverBridge},
    AppPort, AppState, HostAccount, HostProvider, KeyringCredentialStore, TauriEventSink,
};

/// Assembles the host's state over `data_dir`: the same pieces in the same order for
/// every host shape, windowed or headless. The caller owns the three things a shape can
/// differ in: where the data lives, where surface changes are signalled, and where the
/// draft-pool landing is signalled.
///
/// # Errors
///
/// Fails when the data directory cannot be made, the runtime cannot start, the stored
/// accounts cannot be read, or the engine cannot open its store: nothing a host shape
/// could run over.
pub(crate) fn assemble_state(
    data_dir: &Path,
    observer: Arc<dyn astra_app::AppObserver>,
    agent_sink: Arc<dyn AgentEventSink>,
    draft_sink: Arc<dyn astra_app::DraftPoolSink>,
) -> Result<AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let runtime = build_runtime()?;
    let keyring = KeyringCredentialStore::new();
    let connections = Arc::new(AccountDials::load(data_dir)?);

    let accounts = runtime.block_on(|| dial_stored(&connections));
    let (accounts, contacts_legs) = split_dialled(accounts);
    let engine_path = data_dir.join(ENGINE_STORE_FILE);
    // The AI ledgers and the agent: one store the host, the sidecar's
    // reported calls, and the CLI's audited verbs all append to, the
    // background client over the engine's TLS policy, and the relay that
    // hosts the sidecar with the chat tier's endpoint delivered in band.
    // The store and client are built before the app core boots so the
    // incoming AI pass rides the same client from the first refresh.
    let ai_store = Arc::new(astra_ai::AiStore::open(&data_dir.join(AI_STORE_FILE))?);
    let llm = build_llm_client(&ai_store);
    let ai_pass = crate::ai::incoming_pass(
        llm.clone(),
        Arc::clone(&ai_store),
        astra_ai::DlpPolicy::from_env(),
        Some(draft_sink),
    );
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
            ai_pass,
        )
    })?;
    // The contacts legs the dials handed back, installed beside the accounts
    // they belong to before any people dispatch can run: the reads work over
    // the already-synced store either way, and the writes have nowhere to go
    // until a leg stands.
    for (account, leg) in contacts_legs {
        core.install_contacts(&account, leg);
    }
    let model = llm.as_ref().and_then(|client| chat_model_config(client));
    let agent_relay = agent::build_relay(data_dir, model, agent_sink, Arc::clone(&ai_store));

    Ok(AppState {
        app: core as Arc<dyn AppPort>,
        agent: agent_relay,
        llm,
        connections,
        credential_store: Arc::new(keyring),
        runtime,
    })
}

/// The AI ledgers' file name, under the data directory: the same store the
/// CLI opens, so both writers land in one ledger.
const AI_STORE_FILE: &str = "ai.sqlite";

/// Builds the background LLM client when the tier table and the TLS
/// transport resolve: the registry from the `ASTRA_LLM_*` tier lists, the
/// provider keys read once out of the environment, and the ledger store.
/// A malformed tier list or an unbuilt TLS stack leaves the client off
/// with a warning, never a failed boot.
fn build_llm_client(store: &Arc<astra_ai::AiStore>) -> Option<Arc<crate::LlmClient>> {
    let registry = match astra_ai::TierTable::from_env() {
        Ok(registry) => registry,
        Err(e) => {
            log::warn!(
                "boot: the llm tier table is unusable; the background client stays off: {e}"
            );
            return None;
        }
    };
    let tls = match engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system()) {
        Ok(tls) => tls,
        Err(e) => {
            log::warn!(
                "boot: the llm TLS policy is unusable; the background client stays off: {e}"
            );
            return None;
        }
    };
    let http = match tls.reqwest_builder().build() {
        Ok(http) => http,
        Err(e) => {
            log::warn!(
                "boot: the llm HTTP client is unusable; the background client stays off: {e}"
            );
            return None;
        }
    };
    Some(Arc::new(crate::LlmClient::new(
        http,
        registry,
        Arc::clone(store),
    )))
}

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

/// Dials every registered account, mail half first and dav legs beside it.
/// A mail dial that fails skips the account with a warning: one stale
/// password must not block the others, and the app opens with whatever
/// connected. A dav dial that fails keeps the account standing on its mail
/// half alone, inside [`dial_dav_with`].
async fn dial_stored(dials: &AccountDials) -> Vec<Dialled> {
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
            Ok(providers) => {
                let account = HostAccount {
                    id: account_id,
                    providers,
                    identity: EmailAddress::new(email),
                };
                let (account, contacts) = dial_dav(&config, account).await;
                accounts.push(Dialled { account, contacts });
            }
            Err(err) => {
                log::warn!("boot: an account could not be connected and was skipped: {err}");
            }
        }
    }
    accounts
}

/// One dialled account as the boot holds it: the account with every dialled
/// provider installed, and the contacts leg to install beside it once the
/// app core exists.
struct Dialled {
    /// The account as the app installs it: the mail providers first, the
    /// appended calendar provider after.
    account: HostAccount,
    /// The dialled contacts leg, when the account has one.
    contacts: Option<HostContactsLeg>,
}

/// The contacts leg type the boot dials and installs: a CardDAV provider
/// boxed behind the engine's contacts domain, the one shape the mail-generic
/// app core can drive.
pub(crate) type HostContactsLeg = Box<dyn engine_api::ContactsProvider>;

/// Splits the dialled accounts into the list the app core boots over and the
/// contacts legs to install after it: the legs cannot ride the accounts
/// (their providers list is the mail-and-calendar surface), and they cannot
/// be installed before the core exists.
fn split_dialled(dialled: Vec<Dialled>) -> (Vec<HostAccount>, Vec<(AccountId, HostContactsLeg)>) {
    let mut accounts = Vec::new();
    let mut legs = Vec::new();
    for dialled in dialled {
        if let Some(leg) = dialled.contacts {
            legs.push((dialled.account.id.clone(), leg));
        }
        accounts.push(dialled.account);
    }
    (accounts, legs)
}

/// Dials the account's dav legs through the real connect and folds them
/// into `account`.
async fn dial_dav(
    config: &astra_account::AccountConfig,
    account: HostAccount,
) -> (HostAccount, Option<HostContactsLeg>) {
    dial_dav_with(config, account, || {
        astra_account::connect_dav_providers(config)
    })
    .await
}

/// Folds a dialled dav pair into `account`: the CalDAV provider appended
/// after the mail providers (the calendar paths pick it by capability,
/// wherever in the list it stands), the CardDAV provider handed back as the
/// contacts leg to install. A failed dial leaves the account standing on
/// its mail half alone, logged: a server that splits them or a section that
/// names an unreachable origin costs the account neither its mail nor the
/// boot. An account with no dav section dials nothing at all.
async fn dial_dav_with<F, Fut>(
    config: &astra_account::AccountConfig,
    mut account: HostAccount,
    dial: F,
) -> (HostAccount, Option<HostContactsLeg>)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<astra_account::DavProviders, astra_account::AccountError>>,
{
    if config.dav.is_none() {
        return (account, None);
    }
    match dial().await {
        Ok(dav) => {
            if let Some(caldav) = dav.caldav {
                account.providers.push(caldav);
            }
            (account, dav.carddav)
        }
        Err(err) => {
            log::warn!(
                "boot: an account's dav legs could not be connected; the mail half stands alone: {err}"
            );
            (account, None)
        }
    }
}

/// The `setup` hook: resolves the shell's data directory, assembles the host's state
/// over it, and registers it as the managed state the command surface reads. The
/// assembly itself is [`assemble_state`], shared with the headless host shape.
///
/// # Errors
///
/// Fails when the data directory cannot be resolved or the state cannot be assembled:
/// nothing the shell could show a window over.
pub(crate) fn setup_host(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = handle
        .path()
        .app_data_dir()
        .map_err(|err| format!("the app data directory could not be resolved: {err}"))?;
    let observer: Arc<dyn astra_app::AppObserver> = Arc::new(DebouncedObserver::new(
        ObserverBridge::new(Box::new(TauriEventSink::new(handle.clone()))),
    ));
    let agent_sink: Arc<dyn AgentEventSink> = Arc::new(crate::agent::TauriAgentSink::new(handle));
    let draft_sink: Arc<dyn astra_app::DraftPoolSink> =
        Arc::new(crate::ai::DraftPoolEventSink::new(app.handle().clone()));
    let state = assemble_state(&data_dir, observer, agent_sink, draft_sink)?;

    log::info!(
        "boot: the app core is up over {} account(s)",
        state.connections.ids().len(),
    );
    app.manage(state);
    Ok(())
}

#[cfg(test)]
#[path = "boot_tests.rs"]
mod tests;
