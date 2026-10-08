// SPDX-License-Identifier: GPL-3.0-only
//! The Astra Mail desktop host.
//!
//! [`run`] assembles the Tauri shell: the single-instance guard first, then
//! the deep-link (mailto), opener, dialog, and notification plugins, over the
//! frameless main window `tauri.conf.json` declares. The managed state and
//! the exit hook are placeholders; wiring the ports and commands onto them is
//! the next task's work. The app's async runtime lives in [`runtime`].

pub mod runtime;

use tauri::{Emitter, Manager};

/// The state the shell manages. Empty in the scaffold: the ports and the
/// command surface that fill it arrive with the wiring task.
#[derive(Debug, Default)]
pub struct AppState {}

/// Runs the desktop application: registers the plugin set, builds the Tauri
/// context, and drives the event loop until the main window closes.
///
/// The single-instance plugin must be registered first: it owns the
/// first-launch decision that every later plugin's initialisation runs under,
/// and a second launch reaches the app only through its callback, which
/// focuses the existing window and hands it the launch arguments.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
            let _ = app.emit("single-instance-args", argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(setup)
        .build(tauri::generate_context!())
        .expect("the tauri application builds")
        .run(on_run_event);
}

/// The `setup` hook: registers the managed state on the app handle, where the
/// command surface reads it back through `State<'_, AppState>`.
// The `Result` return is the signature Tauri's `.setup()` demands; the
// scaffold's body cannot fail yet.
#[allow(clippy::unnecessary_wraps)]
fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(AppState::default());
    Ok(())
}

/// The exit hook: reached exactly once on every exit path, after the windows
/// are gone but while the managed state is still alive.
// `.run()` hands the event by value; the hook owns what it inspects.
#[allow(clippy::needless_pass_by_value)]
fn on_run_event(_app: &tauri::AppHandle, event: tauri::RunEvent) {
    // The scaffold has nothing to stop yet; the app runtime's shutdown goes
    // here once the wiring task manages one.
    if let tauri::RunEvent::Exit = event {}
}
