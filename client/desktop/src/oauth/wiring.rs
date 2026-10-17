// SPDX-License-Identifier: GPL-3.0-only
//! The sign-in surface's Tauri-touching wiring: the pieces the session
//! module named as its deferrals, split from the command module for the
//! file cap. Four adapters live here: the opener-backed
//! [`SystemBrowser`] the app setup installs ([`opener_browser`]), the
//! deep-link capture's filter and registration
//! ([`oauth_callback`], [`forward_oauth_callbacks`]), the surface for
//! hosts with no browser and no capture ([`offline_surface`]), and the
//! Windows argv forwarding in the single-instance callback
//! ([`forward_launch_arguments`], without which a running app on that
//! platform never sees its own redirect). Only the browser has no
//! unit-test shape: the filter's contract is pinned by the session
//! tests, the rest by the session and command tests.

use std::sync::Arc;

use tokio::sync::mpsc;

use super::commands::SignInSurface;
use super::session::{SystemBrowser, OAUTH_SCHEME};

/// The system browser over the shell's opener: the production seam the
/// sign-in wiring installs; the platform picks the default browser.
struct OpenerBrowser {
    /// The shell handle the opener plugin reads.
    handle: tauri::AppHandle,
}

impl SystemBrowser for OpenerBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        self.handle
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|err| err.to_string())
    }
}

/// The opener-backed browser the app setup installs.
#[must_use]
pub(crate) fn opener_browser(handle: &tauri::AppHandle) -> Arc<dyn SystemBrowser> {
    Arc::new(OpenerBrowser {
        handle: handle.clone(),
    })
}

/// The surface for hosts with no browser and no capture (the headless
/// host, the test fakes): a browser that refuses and a capture that never
/// delivers, so no sign-in can begin there.
#[must_use]
pub(crate) fn offline_surface() -> Arc<SignInSurface> {
    let (tx, rx) = mpsc::unbounded_channel();
    Arc::new(SignInSurface::new(Arc::new(RefusingBrowser), rx, tx))
}

/// A browser no host can open.
struct RefusingBrowser;

impl SystemBrowser for RefusingBrowser {
    fn open(&self, _url: &str) -> Result<(), String> {
        Err("this host has no system browser".to_owned())
    }
}

/// The OAuth-scheme callback among one deep-link delivery's URLs, whole.
///
/// A delivery is a batch (the plugin's event carries every URL the OS
/// handed over in one go); the capture keeps the one URL on
/// [`OAUTH_SCHEME`] and drops the rest, and returns it as the raw string
/// the sign-in hands to the core's parser. It never inspects the query:
/// `code`, `state`, and any provider `error` are `astra-oauth`'s to read.
#[must_use]
pub(crate) fn oauth_callback(urls: &[url::Url]) -> Option<String> {
    urls.iter()
        .find(|url| url.scheme() == OAUTH_SCHEME)
        .map(ToString::to_string)
}

/// Wires the deep-link plugin's event stream into `tx`: every OAuth
/// callback the running app receives is forwarded whole. The sign-in
/// wiring calls this once at setup; it is here, beside the filter it
/// applies, so the capture's production shape lives in one place.
///
/// On Windows, a deep link that arrives while the app is already running
/// reaches the first instance through the single-instance callback's
/// argv, not this event; that forwarding (`DeepLink::handle_cli_arguments`)
/// is the sign-in commands' wiring too, recorded in the module doc.
pub(crate) fn forward_oauth_callbacks<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    tx: mpsc::UnboundedSender<String>,
) {
    use tauri_plugin_deep_link::DeepLinkExt;

    app.deep_link().on_open_url(move |event| {
        if let Some(raw) = oauth_callback(&event.urls()) {
            let _ = tx.send(raw);
        }
    });
}

/// Hands the single-instance callback's argv to the deep-link plugin: on
/// Windows a deep link that arrives while the app runs reaches the first
/// instance as a launch argument, and without this hand-off the plugin
/// never emits it, so a running app never sees the redirect its own
/// sign-in opened. The plugin itself checks the argument against the
/// configured schemes, so a plain relaunch's arguments reach nothing.
pub(crate) fn forward_launch_arguments<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    argv: &[String],
) {
    use tauri_plugin_deep_link::DeepLinkExt;
    app.deep_link().handle_cli_arguments(argv.iter());
}
