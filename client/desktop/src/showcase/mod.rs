// SPDX-License-Identifier: GPL-3.0-only
//! The debug showcase boot: a fixture-backed shell for screenshot runs.
//! `ASTRA_SHOWCASE` names the one screen the run photographs; the
//! assembly answers the recorded snapshots the web fixture harness
//! carries, so the production webview bundle runs unmodified against
//! recorded answers and no live account, dial, or engine is involved.
//! The reference's in-memory provider boot stays not taken: it needs
//! unvendored bindings code against a dial-first boot, and a screenshot
//! run does not need real-engine fidelity.
//!
//! The whole module compiles only in debug builds, so a shipped binary
//! cannot have its surfaces swapped for recorded answers by a stray
//! environment variable. The snapshots' dates stand as recorded: no
//! clock is pinned, the screens freeze whatever the fixtures carry.

mod app;
mod backend;

use std::{path::Path, sync::Arc};

use crate::agent::AgentEventSink;
use crate::app_state::AppState;
use crate::runtime::build_runtime;

use app::ShowcaseApp;

/// Which screen a screenshot run photographs. The names are the flag's
/// contract spellings, matched literally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    /// The message list over the recorded mailbox.
    List,
    /// The reading pane open on the recorded message, beside the list.
    Reading,
    /// The settings surface over the recorded settings.
    Settings,
    /// The people surface over the recorded directory.
    People,
}

/// The values that mean off, so `ASTRA_SHOWCASE=0` reads as unset rather
/// than as a screen.
const OFF: [&str; 5] = ["", "0", "false", "no", "off"];

/// Reads one raw value as a screen choice: `Ok(None)` when no showcase
/// stands, the named screen, or the name it was asked for when no
/// surface answers it. An unknown name is refused rather than silently
/// photographed as the list: a well-lit capture of the wrong screen is
/// the failure nothing downstream can detect.
pub(crate) fn screen_from_reader(raw: Option<&str>) -> Result<Option<Screen>, String> {
    match raw
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        None => Ok(None),
        Some(value) if OFF.contains(&value) => Ok(None),
        Some("list") => Ok(Some(Screen::List)),
        Some("reading") => Ok(Some(Screen::Reading)),
        Some("settings") => Ok(Some(Screen::Settings)),
        Some("people") => Ok(Some(Screen::People)),
        Some(other) => Err(other.to_owned()),
    }
}

/// The screen the launch environment names; unset or off means the
/// production boot.
pub(crate) fn requested() -> Result<Option<Screen>, String> {
    screen_from_reader(std::env::var("ASTRA_SHOWCASE").ok().as_deref())
}

/// Assembles the shell's state over the fixture app: the same pieces the
/// production assembly registers, with no dial, no engine store, and no
/// LLM client. The showcase's own corner under the data directory holds
/// the account index, and it names nothing: the dial table stands empty,
/// so neither background loop (the delivery adoption, the calendar
/// timer) finds an account to adopt.
///
/// # Errors
///
/// Fails when the data directory cannot be made, the runtime or the AI
/// ledger cannot start, or a compiled-in fixture cannot be read: nothing
/// a screenshot run could run over.
pub(crate) fn assemble(
    data_dir: &Path,
    screen: Screen,
    agent_sink: Arc<dyn AgentEventSink>,
) -> Result<AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let runtime = build_runtime()?;
    let showcase_dir = data_dir.join("showcase");
    std::fs::create_dir_all(&showcase_dir)?;
    let connections = Arc::new(crate::AccountDials::load(&showcase_dir)?);
    let ai_store = Arc::new(astra_ai::AiStore::open(
        &data_dir.join(crate::boot::AI_STORE_FILE),
    )?);
    let agent = crate::agent::build_relay(data_dir, None, agent_sink, Arc::clone(&ai_store));
    let app = Arc::new(ShowcaseApp::for_screen(screen)?);
    let mcp_server = astra_mcp::server::Server::new(
        Arc::clone(&app) as Arc<dyn astra_mcp::backend::MailBackend>,
        Arc::clone(&ai_store) as Arc<dyn astra_mcp::ledger::ToolLedger>,
    );
    let mcp = Arc::new(crate::mcp::McpHost::new(Arc::new(mcp_server), data_dir));
    let plugins = Arc::new(crate::plugin::PluginRegistry::new(
        data_dir.join("plugins"),
        Vec::new(),
        cfg!(debug_assertions),
    ));

    Ok(AppState {
        app: app as Arc<dyn crate::app_state::AppPort>,
        agent,
        mcp,
        plugins,
        llm: None,
        connections,
        crypto_store: Arc::new(astra_crypto_store::CryptoStore::open_in_memory()?),
        photo_root: showcase_dir.join("contact-photos"),
        credential_store: Arc::new(crate::KeyringCredentialStore::new()),
        oauth: crate::oauth::wiring::offline_surface(),
        refusals: Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
        pending_mailto: std::sync::Mutex::new(None),
        pending_share: std::sync::Mutex::new(None),
        log_path: crate::logging::file_path(data_dir),
        runtime,
    })
}

#[cfg(test)]
#[path = "showcase_tests.rs"]
mod tests;
