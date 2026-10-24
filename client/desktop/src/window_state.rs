// SPDX-License-Identifier: GPL-3.0-only
//! The main window's persisted geometry: size, position, and
//! maximised, hand-rolled over the app data dir (no plugin, the
//! window-state file is not a preference, so the core's preferences
//! document stays the one document it is).
//!
//! The restore discipline, recorded once:
//!
//! - **Size and position apply at creation.** The setup hook applies
//!   them after the shell creates the window and before the event
//!   loop's first paint, so no wrong-size frame ever flashes.
//! - **The maximised state applies after the frame shows.** The page's
//!   first finished load is the moment, because maximising a window
//!   before its frame shows clobbers the restore-down geometry (the
//!   flash this one-shot exists to avoid).
//! - **A missing, unreadable, unparseable, or insane file falls back
//!   to the config defaults** (the fixed 1200 by 800 `tauri.conf.json`
//!   declares), which is also what a first launch stands at.
//! - **The maximised frame's own geometry is what a maximised close
//!   saves.** The OS's restore-down rectangle is not readable through
//!   the shell's API, so a window closed maximised restores maximised
//!   and a later unmaximise stands at the maximised frame's size: the
//!   named cost of the hand-rolled state, accepted over a geometry
//!   guess.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

/// The window-state file's name, under the app data directory beside
/// the stores.
const STATE_FILE: &str = "window-state.json";

/// The main window's label: the one window whose state persists.
const MAIN_WINDOW: &str = "main";

/// The main window's persisted geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WindowState {
    /// The frame's width, in physical pixels.
    pub(crate) width: u32,
    /// The frame's height, in physical pixels.
    pub(crate) height: u32,
    /// The frame's x position, in physical pixels.
    pub(crate) x: i32,
    /// The frame's y position, in physical pixels.
    pub(crate) y: i32,
    /// Whether the frame stood maximised.
    pub(crate) maximised: bool,
}

/// The sane bounds a stored state may hold: a window nobody could size
/// or place is a corrupt file, not a state to restore.
fn sane(state: &WindowState) -> bool {
    (1..=100_000).contains(&state.width)
        && (1..=100_000).contains(&state.height)
        && state.x.abs() <= 100_000
        && state.y.abs() <= 100_000
}

/// Loads the persisted state; a missing, unreadable, unparseable, or
/// insane file answers `None`, the fallback that leaves the window at
/// the config defaults.
pub(crate) fn load(data_dir: &Path) -> Option<WindowState> {
    let body = std::fs::read_to_string(data_dir.join(STATE_FILE)).ok()?;
    let state: WindowState = serde_json::from_str(&body).ok()?;
    sane(&state).then_some(state)
}

/// Persists the state, best effort: a refusal logs, never fails the
/// close that asked for it.
pub(crate) fn save(data_dir: &Path, state: &WindowState) {
    let body = match serde_json::to_string(state) {
        Ok(body) => body,
        Err(err) => {
            log::warn!("window-state: the geometry could not serialise: {err}");
            return;
        }
    };
    if let Err(err) = std::fs::write(data_dir.join(STATE_FILE), body) {
        log::warn!("window-state: the geometry could not persist: {err}");
    }
}

/// The one-shot the maximised restore arms and the page's first
/// finished load consumes.
static PENDING_MAXIMISE: AtomicBool = AtomicBool::new(false);

/// The restore's creation half: applies the persisted size and
/// position to the main window in the setup hook, before the event
/// loop's first paint, and arms the maximised one-shot the page load
/// consumes. A window with no state stands at the config defaults.
pub(crate) fn restore<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, data_dir: &Path) {
    let Some(state) = load(data_dir) else {
        return;
    };
    let _ = window.set_size(tauri::PhysicalSize::new(state.width, state.height));
    let _ = window.set_position(tauri::PhysicalPosition::new(state.x, state.y));
    if state.maximised {
        PENDING_MAXIMISE.store(true, Ordering::Relaxed);
    }
}

/// The restore's page-load half: the main page's first finished load
/// is the moment the frame stands shown, and the armed maximised state
/// applies there.
pub(crate) fn on_page_load<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    payload: &tauri::webview::PageLoadPayload<'_>,
) {
    use tauri::{webview::PageLoadEvent, Manager};
    if payload.event() != PageLoadEvent::Finished || webview.label() != MAIN_WINDOW {
        return;
    }
    if !PENDING_MAXIMISE.swap(false, Ordering::Relaxed) {
        return;
    }
    let app = webview.app_handle();
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.maximize();
    }
}

/// The save's close half: the main window's close request reads the
/// geometry the frame stands in and persists it. Other windows are not
/// the main window's business.
pub(crate) fn save_close<R: tauri::Runtime>(window: &tauri::Window<R>) {
    use tauri::Manager;
    if window.label() != MAIN_WINDOW {
        return;
    }
    let Ok(data_dir) = window.app_handle().path().app_data_dir() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let Ok(position) = window.outer_position() else {
        return;
    };
    save(
        &data_dir,
        &WindowState {
            width: size.width,
            height: size.height,
            x: position.x,
            y: position.y,
            maximised: window.is_maximized().unwrap_or(false),
        },
    );
}

#[cfg(test)]
#[path = "window_state_tests.rs"]
mod tests;
