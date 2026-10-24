// SPDX-License-Identifier: GPL-3.0-only
//! The share-target entry's host leg: the mailto channel's sibling.
//! Files the OS or a person handed the process cross into a compose
//! prefill through one event, exactly the way a raw `mailto:` link
//! does.
//!
//! The Windows judgement, recorded once: the OS share contract (the
//! Share UI a packaged app registers as a target for) needs package
//! identity, and the desktop build is an unpackaged Win32 app, so no
//! runtime registration can make it a share target. The honest
//! hand-off that ships is the file-association "open with" road: file
//! arguments reach the process (at launch, or through the
//! single-instance guard when the app already runs), the host filters
//! them at the door (unsafe names and types dropped, the row's own
//! discipline), and the survivors cross on the share channel the
//! composer's prefill opens from. The packaged share target and the
//! byte-staging read seam are the row's named remainder.
//!
//! The path crosses beside the name because the webview holds no file
//! system of its own: the string does nothing until a read seam
//! stands, and no such seam ships here.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::app_state::AppState;

/// The event the shared files cross to the web on, the name the
/// composer's listener subscribes to (the mailto sibling).
pub(crate) const SHARE_OPEN_EVENT: &str = "share_open";

/// The main window's label, the one page whose load drains the held
/// launch files.
const MAIN_WINDOW: &str = "main";

/// One shared file as the channel carries it: the name the compose
/// prefill shows beside the path a later byte read resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SharedFile {
    /// The file's own name, as the prefill shows it.
    pub(crate) name: String,
    /// The file's path, as the OS handed it over.
    pub(crate) path: String,
}

/// The extensions the OS would execute on a double-click: the share
/// hand-off drops them at the door, because an executable mailed
/// through a share is the classic phishing shape.
const UNSAFE_TYPES: [&str; 8] = ["exe", "bat", "cmd", "com", "scr", "msi", "ps1", "vbs"];

/// One candidate name's sanity: a plain file name, no control
/// characters, none of the two names a path resolves on its own.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.chars().any(char::is_control)
}

/// The share filter: keeps the hand-off entries that stand for
/// existing plain files whose names and types are safe, dropping
/// everything else (directories, executables, names that are not
/// plain, paths that name nothing).
pub(crate) fn shared_files(paths: impl IntoIterator<Item = PathBuf>) -> Vec<SharedFile> {
    paths
        .into_iter()
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(safe_name)
        })
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_none_or(|ext| !UNSAFE_TYPES.contains(&ext.to_ascii_lowercase().as_str()))
        })
        .map(|path| SharedFile {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.display().to_string(),
        })
        .collect()
}

/// Holds shared files in the launch hold, the mailto hold's sibling:
/// an empty hand-off holds nothing, and a later hold replaces what
/// stood.
pub(crate) fn hold_files(state: &AppState, files: Vec<SharedFile>) {
    if files.is_empty() {
        return;
    }
    *state
        .pending_share
        .lock()
        .expect("the share hold is not poisoned") = Some(files);
}

/// Takes the held shared files: the page-load drain consumes the hold
/// once, and every later drain finds it empty.
pub(crate) fn drain(state: &AppState) -> Option<Vec<SharedFile>> {
    state
        .pending_share
        .lock()
        .expect("the share hold is not poisoned")
        .take()
}

/// Emits the shared files on the share channel.
fn emit_shared<R: tauri::Runtime>(app: &tauri::AppHandle<R>, files: &[SharedFile]) {
    let _ = app.emit(SHARE_OPEN_EVENT, files);
}

/// The launch hold: the files the process was started by, filtered at
/// the door. Called at setup, beside the mailto launch hold.
pub(crate) fn hold_launch_files(state: &mut AppState) {
    let files = shared_files(std::env::args_os().skip(1).map(PathBuf::from));
    hold_files(state, files);
}

/// The second launch's hand-off: the single-instance guard forwards
/// the argv of a launch that reached the running app, and its file
/// entries cross at once, the page already standing.
pub(crate) fn forward_files<R: tauri::Runtime>(app: &tauri::AppHandle<R>, argv: &[String]) {
    let files = shared_files(argv.iter().skip(1).map(PathBuf::from));
    if files.is_empty() {
        return;
    }
    emit_shared(app, &files);
}

/// The drain: the main page finishing its load is the first moment
/// the web's listener stands (the mailto drain's sibling); the held
/// files cross once and every later load finds the hold empty.
pub(crate) fn on_page_load<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    payload: &tauri::webview::PageLoadPayload<'_>,
) {
    use tauri::webview::PageLoadEvent;
    if payload.event() != PageLoadEvent::Finished || webview.label() != MAIN_WINDOW {
        return;
    }
    let app = webview.app_handle();
    let state = app.state::<AppState>();
    if let Some(files) = drain(&state) {
        emit_shared(webview.app_handle(), &files);
    }
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
