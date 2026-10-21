// SPDX-License-Identifier: GPL-3.0-only
//! The Astra Mail desktop host.
//!
//! [`run`] assembles the Tauri shell: the single-instance guard first, then the
//! deep-link (mailto), opener, dialog, and notification plugins, over the frameless
//! main window `tauri.conf.json` declares, with the IPC command surface registered
//! and the boot hook that builds the managed state behind it. The app's async
//! runtime lives in [`runtime`], the observer port and its rate limiter in
//! `observer`, the credential store port in `credential_store`, the background
//! LLM client in `ai`, and the exit hook that stops sync in `exit`.

pub mod runtime;

mod account_ops;
mod agent;
mod ai;
mod app_state;
mod boot;
mod commands;
mod credential_store;
mod detection;
mod dial_table;
mod exit;
mod headless;
mod mailto;
mod mcp;
mod oauth;
mod observer;
mod plugin;

// The command-surface test fixtures: one compiled copy of the fake app, the
// credential store, and the state builder, at the crate root so every command
// test module (the command tests, the reading tests they split off, and the
// drafts, sending, and compose tests) includes the same module rather than
// its own.
#[cfg(test)]
mod commands_fakes;

// The one log capture the test binary shares, for the tests whose contract
// is what a code path logs rather than what it answers.
#[cfg(test)]
mod log_capture;

use tauri::{Emitter, Manager};

pub(crate) use account_ops::HostAccountStore;
pub(crate) use agent::SidecarRelay;
pub use ai::{
    incoming_pass, repair_json, LlmAiComplete, LlmAiJudge, LlmClient, LlmEndpoint, LlmError,
};
pub(crate) use app_state::{AppPort, AppState, HostAccount, HostProvider};
pub use credential_store::CredentialStoreError;
pub(crate) use credential_store::{AccountCredentialStore, KeyringCredentialStore};
pub(crate) use dial_table::AccountDials;
pub use headless::run_headless_slice;
pub use observer::{SurfaceChanged, SurfaceEventSink, TauriEventSink};
pub(crate) use runtime::{build_runtime, AppRuntime};

/// Runs the desktop application: registers the plugin set and the command surface,
/// builds the Tauri context, and drives the event loop until the main window
/// closes.
///
/// The single-instance plugin must be registered first: it owns the first-launch
/// decision that every later plugin's initialisation runs under, and a second
/// launch reaches the app only through its callback, which focuses the existing
/// window and hands it the launch arguments.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
            // A deep link that reached the already-running app arrives as
            // this argv on Windows; the hand-off makes the deep-link
            // plugin emit it, which is the only road an OAuth redirect
            // takes back into a running instance.
            oauth::wiring::forward_launch_arguments(app, &argv);
            let _ = app.emit("single-instance-args", argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .on_page_load(mailto::on_page_load)
        .invoke_handler(tauri::generate_handler![
            commands::dispatch,
            commands::mailbox_list,
            commands::sync_progress,
            commands::reading,
            commands::attachments::save_attachment,
            commands::drafts::draft_status,
            commands::drafts::draft_resume,
            commands::sending::sending,
            commands::compose::compose_request,
            commands::compose::take_compose_request,
            commands::ai::draft_chips,
            commands::ai::summarise,
            commands::ai::style_profile,
            commands::ai::chips_dismiss,
            commands::ai::compose_from_draft,
            commands::ai::knowledge_list,
            commands::ai::knowledge_extract,
            commands::ai::knowledge_forget,
            commands::ai::snippet_list,
            commands::ai::snippet_search,
            commands::ai::compose_autocomplete,
            commands::ai::translate,
            commands::ai_docs::digest_generate,
            commands::ai_docs::report_generate,
            commands::ai_docs::latest_document,
            commands::ai_docs::split_map,
            commands::calendar::calendar,
            commands::calendar::calendar_brief_generate,
            commands::people::people,
            commands::people::people_card,
            commands::people::people_suggestions,
            commands::people::people_avatar,
            commands::people::people_photo,
            commands::settings::settings,
            commands::mcp::mcp_status,
            commands::mcp::mcp_set_enabled,
            commands::mcp::mcp_set_allowed,
            commands::add_account,
            commands::jmap::add_jmap_account,
            commands::imap_test_connection,
            commands::detect_account_settings,
            oauth::commands::oauth_offer,
            oauth::commands::oauth_begin,
            oauth::commands::oauth_cancel,
            oauth::jmap_commands::jmap_oauth_offered,
            oauth::jmap_commands::jmap_oauth_begin,
            agent::commands::agent_prompt,
            agent::commands::agent_abort,
            agent::commands::agent_status,
            agent::commands::permission_response,
            commands::plugin::plugin_list,
            commands::plugin::plugin_enable,
            commands::plugin::plugin_disable,
            commands::plugin::plugin_approve,
            commands::plugin::plugin_call
        ])
        .setup(boot::setup_host)
        .build(tauri::generate_context!())
        .expect("the tauri application builds")
        .run(exit::on_run_event);
}
