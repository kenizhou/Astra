// SPDX-License-Identifier: GPL-3.0-only
//! The mailto entry's host leg: the deep-link plugin's deliveries and the
//! web's reading-pane asks both hand a raw `mailto:` link to the app
//! core, whose parse answers the prefill the composer opens from,
//! emitted to the web as one event. Three roads meet here: the plugin's
//! event (a link that reaches the running app, which on Windows includes
//! the second launch's argv the single-instance callback hands the
//! plugin), the plugin's launch argument (a link that started the app,
//! consumed by the plugin before any listener stood, so its current
//! value is held until the main page has loaded and the web's listener
//! can hear), and the reading pane's own anchor clicks, which cross as
//! an ask rather than a parse because protocol parsing belongs to the
//! engine. The OAuth capture's filter is untouched: this listener reads
//! the same deliveries and keeps only the mailto scheme, exactly as the
//! OAuth wiring keeps only its own.

use std::sync::Arc;

use serde::Deserialize;
use tauri::{Emitter, Manager};

use astra_viewmodel::MailtoOpen;

use crate::app_state::{AppPort, AppState};

/// The event the parsed prefill crosses to the web on, the name the
/// composer's listener subscribes to.
pub(crate) const MAILTO_OPEN_EVENT: &str = "mailto_open";

/// The event the reading pane hands a clicked mailto link over on, raw:
/// the host owns the parse, so nothing but the link itself crosses.
pub(crate) const MAILTO_REQUEST_EVENT: &str = "mailto_request";

/// The main window's label, the one page whose load drains the held
/// launch link.
const MAIN_WINDOW: &str = "main";

/// The mailto-scheme link among one deep-link delivery's URLs, whole.
///
/// The mirror of the OAuth capture's filter: a delivery is a batch (the
/// plugin's event carries every URL the OS handed over in one go), this
/// keeps the one `mailto:` URL and drops the rest, and hands it on as
/// the raw string the app core's parser reads. A delivery that carries
/// no mailto link answers `None`, so nothing opens and no other scheme's
/// delivery is disturbed.
#[must_use]
pub(crate) fn mailto_link(urls: &[url::Url]) -> Option<String> {
    urls.iter()
        .find(|url| url.scheme() == "mailto")
        .map(ToString::to_string)
}

/// The reading pane's ask: the raw link, nothing the host parsed.
#[derive(Debug, Deserialize)]
struct MailtoRequest {
    uri: String,
}

/// Opens the link through the app core and hands the prefill it answers
/// with to `emit`. A `None` answer, no account standing or the link not
/// a mailto link at all, emits nothing: the host never opens a composer
/// over a link it could not read.
async fn open_and_emit<F>(app: &Arc<dyn AppPort>, uri: &str, emit: F)
where
    F: FnOnce(MailtoOpen),
{
    if let Some(open) = app.open_mailto(uri).await {
        emit(open);
    }
}

/// Spawns one link's open onto the app's runtime: the prefill the core
/// answers with crosses to the web as the [`MAILTO_OPEN_EVENT`] event.
fn open_link<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    core: &Arc<dyn AppPort>,
    driver: &tokio::runtime::Handle,
    uri: &str,
) {
    let core = Arc::clone(core);
    let emitter = app.clone();
    let uri = uri.to_owned();
    driver.spawn(async move {
        open_and_emit(&core, &uri, |open| {
            let _ = emitter.emit(MAILTO_OPEN_EVENT, open);
        })
        .await;
    });
}

/// Wires the mailto roads that need the standing state: the plugin's
/// event stream and the reading pane's asks. Called once at setup, after
/// the state is managed; the OAuth capture's own registration is
/// untouched beside it.
pub(crate) fn listen<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Listener;
    use tauri_plugin_deep_link::DeepLinkExt;

    let state = app.state::<AppState>();
    let core = Arc::clone(&state.app);
    let driver = state.runtime.handle();

    let event_app = app.clone();
    let event_core = Arc::clone(&core);
    let event_driver = driver.clone();
    app.deep_link().on_open_url(move |event| {
        if let Some(uri) = mailto_link(&event.urls()) {
            open_link(&event_app, &event_core, &event_driver, &uri);
        }
    });

    let ask_app = app.clone();
    let ask_core = Arc::clone(&core);
    let ask_driver = driver;
    app.listen(MAILTO_REQUEST_EVENT, move |event| {
        let Ok(ask) = serde_json::from_str::<MailtoRequest>(event.payload()) else {
            return;
        };
        open_link(&ask_app, &ask_core, &ask_driver, &ask.uri);
    });
}

/// Holds the launch mailto, when the process was started by one: the
/// plugin consumed the launch argument at its own init, before any
/// listener stood, so the current value it kept is read here and held
/// for the page-load drain. A launch without a mailto link holds
/// nothing.
pub(crate) fn hold_launch_link<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &mut AppState) {
    use tauri_plugin_deep_link::DeepLinkExt;
    let Ok(Some(urls)) = app.deep_link().get_current() else {
        return;
    };
    if let Some(uri) = mailto_link(&urls) {
        *state
            .pending_mailto
            .lock()
            .expect("the launch link hold is not poisoned") = Some(uri);
    }
}

/// The held launch link's drain: the main page finishing its load is the
/// first moment the web's listener stands, so the open the launch link
/// asks for runs there, once; every later load finds the hold empty and
/// opens nothing.
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
    let Some(uri) = state
        .pending_mailto
        .lock()
        .expect("the launch link hold is not poisoned")
        .take()
    else {
        return;
    };
    open_link(app, &Arc::clone(&state.app), &state.runtime.handle(), &uri);
}

#[cfg(test)]
#[path = "mailto_tests.rs"]
mod tests;
