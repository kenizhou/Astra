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

use std::{path::Path, sync::Arc};

use tauri::Manager;

use astra_app::SpawnedTask;

use crate::{
    agent::{self, chat_model_config, AgentEventSink},
    build_runtime,
    observer::{DebouncedObserver, ObserverBridge},
    AccountDials, AppPort, AppState, HostAccountStore, KeyringCredentialStore, TauriEventSink,
};

// The stored-account restore's dial half: every family's stored document
// into a host-connected account, and the on-demand folder binder.
mod stored;

use stored::{dial_stored, split_dialled, DialConnector};

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
    // The standing grant refusals: what the boot dial learns about every
    // Microsoft account's grant, the state the refusal surfaces render.
    // Built before the dial so the dial can write it, shared with the
    // account store the settings rows read it through and the sign-in
    // completion that rewrites it.
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let accounts = runtime.block_on(|| {
        dial_stored(
            &connections,
            &credentials,
            crate::oauth::graph_add::production_dial(),
            crate::oauth::google_add::production_google_dial(),
            &refusals,
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
            Some(Box::new(DialConnector::new(Arc::clone(&connections)))),
            observer,
            spawn,
            ai_pass,
        )
    })?;
    // The settings seam: the account store the settings surface reads the
    // dial table through and writes it back over, installed before the
    // state hands anything to a command, so every settings dispatch from
    // the first finds it standing. The store reads the standing grant
    // refusals beside the configs, so the rows the snapshot renders carry
    // what the boot dial learned.
    core.install_account_ops(Arc::new(HostAccountStore::new(
        Arc::clone(&connections),
        Arc::clone(&credentials),
        Arc::clone(&refusals),
    )));
    // Prime the settings snapshot: nothing republishes it until a settings
    // intent lands, so without this the first pull after a launch answers
    // the empty default, grant states and account rows included.
    runtime.block_on(|| {
        core.dispatch(astra_app::Intent::Settings(
            astra_app::SettingsIntent::Refresh,
        ))
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
        refusals,
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
        crate::oauth::wiring::opener_browser(app.handle()),
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
