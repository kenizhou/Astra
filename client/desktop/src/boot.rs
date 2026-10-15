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

use std::{future::Future, path::Path, sync::Arc};

use async_trait::async_trait;
use engine_api::{AccountId, EmailAddress};
use tauri::Manager;

use astra_app::{MailboxConnector, SpawnedTask};

use crate::{
    agent::{self, chat_model_config, AgentEventSink},
    build_runtime,
    observer::{DebouncedObserver, ObserverBridge},
    AccountDials, AppPort, AppState, HostAccount, HostAccountStore, HostProvider,
    KeyringCredentialStore, TauriEventSink,
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
    oauth: Arc<crate::oauth::commands::SignInSurface>,
) -> Result<AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let runtime = build_runtime()?;
    let credentials: Arc<dyn crate::credential_store::AccountCredentialStore> =
        Arc::new(KeyringCredentialStore::new());
    let connections = Arc::new(AccountDials::load(data_dir)?);

    let accounts = runtime.block_on(|| {
        dial_stored(
            &connections,
            &credentials,
            crate::oauth::graph_add::production_dial(),
        )
    });
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
    // The settings seam: the account store the settings surface reads the
    // dial table through and writes it back over, installed before the
    // state hands anything to a command, so every settings dispatch from
    // the first finds it standing.
    core.install_account_ops(Arc::new(HostAccountStore::new(
        Arc::clone(&connections),
        Arc::clone(&credentials),
    )));
    // The contacts legs the dials handed back, installed beside the accounts
    // they belong to before any people dispatch can run: the reads work over
    // the already-synced store either way, and the writes have nowhere to go
    // until a leg stands.
    for (account, leg) in contacts_legs {
        core.install_contacts(&account, leg);
    }
    let model = llm.as_ref().and_then(|client| chat_model_config(client));
    let agent_relay = agent::build_relay(data_dir, model, agent_sink, Arc::clone(&ai_store));

    // The MCP listener: the server over the typed core (the AppBackend
    // port is generic over the provider, so it takes the core before the
    // AppPort erasure hands it to the state below), the tool-call ledger
    // from the AI store, and the host over the data directory. Off and
    // exposing nothing at every boot: the enable flag and the account
    // allowlist are session state the panel owns, never a preference
    // the document persists.
    let mcp_server = astra_mcp::server::Server::new(
        Arc::clone(&core) as Arc<dyn astra_mcp::backend::MailBackend>,
        Arc::clone(&ai_store) as Arc<dyn astra_mcp::ledger::ToolLedger>,
    );
    let mcp = Arc::new(crate::mcp::McpHost::new(Arc::new(mcp_server), data_dir));

    Ok(AppState {
        app: core as Arc<dyn AppPort>,
        agent: agent_relay,
        mcp,
        llm,
        connections,
        photo_root: engine_path
            .with_file_name(format!("{ENGINE_STORE_FILE}.blobs"))
            .join("contact-photos"),
        credential_store: credentials,
        oauth,
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

/// Dials every registered account, mail half first and dav legs beside
/// it. A mail dial that fails skips the account with a warning: one stale
/// password must not block the others, and the app opens with whatever
/// connected. A dav dial that fails keeps the account standing on its mail
/// half alone, inside [`dial_dav_with`]. A Graph account dials through
/// `graph_dial`, the same free function the sign-in's completion uses,
/// over a sink-backed token source built from its stored grant.
async fn dial_stored(
    dials: &Arc<AccountDials>,
    credentials: &Arc<dyn crate::credential_store::AccountCredentialStore>,
    graph_dial: crate::oauth::graph_add::GraphDial,
) -> Vec<Dialled> {
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
        if config.imap.is_none() && config.graph.is_some() {
            match dial_graph_stored(dials, credentials, &graph_dial, &account_id, &config).await {
                Ok(account) => accounts.push(Dialled {
                    account,
                    contacts: None,
                }),
                Err(err) => {
                    log::warn!("boot: an account could not be connected and was skipped: {err}");
                }
            }
            continue;
        }
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

/// Dials one stored Graph account: the sink-backed token source over the
/// stored refresh token, then the shared dial. The account joins with its
/// calendar appended after its mail providers, the shape the
/// capability-scoped calendar selection expects; a calendar that failed
/// costs the boot nothing, its error carried in the dial's outcome for
/// the log.
async fn dial_graph_stored(
    dials: &Arc<AccountDials>,
    credentials: &Arc<dyn crate::credential_store::AccountCredentialStore>,
    graph_dial: &crate::oauth::graph_add::GraphDial,
    account_id: &AccountId,
    config: &astra_account::AccountConfig,
) -> Result<HostAccount, astra_account::AccountError> {
    let graph = config
        .graph
        .as_ref()
        .ok_or(astra_account::AccountError::NoImap)?;
    // The stored grant dials through the same sink the sign-in's
    // completion registers: a rotation at boot lands in the entry and the
    // store, and a stored source adopts whatever this process already
    // holds for the account.
    let sink =
        crate::oauth::graph_sink::graph_token_sink(Arc::clone(dials), Arc::clone(credentials));
    let tokens = astra_account::GraphTokenSource::new(
        &graph.provider_config(),
        account_id.clone(),
        graph.refresh_token.expose(),
        Some(sink),
        astra_account::CredentialOrigin::Stored,
    )?;
    let dialled = graph_dial(account_id, tokens, config).await?;
    if let Some(err) = &dialled.calendar_error {
        if dialled.calendar_reauth_required {
            log::warn!("boot: a graph calendar needs a fresh sign-in; the agenda waits: {err}");
        } else {
            log::warn!("boot: a graph calendar failed to dial; the mail stands: {err}");
        }
    }
    let mut providers = dialled.mail;
    if let Some(calendar) = dialled.calendar {
        providers.push(calendar);
    }
    Ok(HostAccount {
        id: account_id.clone(),
        providers,
        identity: EmailAddress::new(graph.email.clone()),
    })
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
    // The OAuth capture: the deep-link plugin's OAuth-scheme URLs are
    // forwarded whole into the sign-in surface, whose browser is the
    // shell's opener. Registered here, once, before the state the sign-in
    // commands read stands.
    let (oauth_tx, oauth_rx) = tokio::sync::mpsc::unbounded_channel();
    crate::oauth::session::forward_oauth_callbacks(app.handle(), oauth_tx.clone());
    let oauth = Arc::new(crate::oauth::commands::SignInSurface::new(
        crate::oauth::commands::opener_browser(app.handle()),
        oauth_rx,
        oauth_tx,
    ));
    let state = assemble_state(&data_dir, observer, agent_sink, draft_sink, oauth)?;

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
