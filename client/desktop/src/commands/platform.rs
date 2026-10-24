// SPDX-License-Identifier: GPL-3.0-only
//! The platform half of the command surface: the default-mail
//! registration verb.
//!
//! The ask-once discipline, recorded once: the OFFER state (whether
//! the app has ever asked to become the default mail app) lives
//! host-side, in the app data dir beside this verb, not as a core
//! preference. The acceptance is an OS registration the host owns and
//! the core's settings block has no other use for, so the latch
//! belongs where its only reader (the asking surface) reaches it. The
//! latch file lands with that surface, read and written beside this
//! verb; what ships here is the verb itself. The OS-level proof that
//! the registration took (the live mailto hand-off) waits on an
//! installed build exactly as the mailto row already records.
//!
//! A child of [`super`], under the same split the other command
//! halves made for the line cap.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// The mailto scheme the default-mail registration claims.
const MAILTO: &str = "mailto";

/// The host leg's seam: the OS's default-handler registration, so the
/// command's tests drive it without a registry.
pub(crate) trait DefaultMailHost {
    /// Whether this app already stands as the OS's mailto handler.
    fn is_registered(&self) -> Result<bool, String>;
    /// Claims the mailto association for this app.
    fn register(&self) -> Result<(), String>;
}

/// The production host: the deep-link plugin's runtime registration,
/// the same registration the installed build's manifest declares
/// statically.
pub(crate) struct DeepLinkHost<'a> {
    /// The shell handle the plugin answers through.
    app: &'a tauri::AppHandle,
}

impl<'a> DeepLinkHost<'a> {
    /// The production host over `app`'s deep-link plugin.
    pub(crate) fn new(app: &'a tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl DefaultMailHost for DeepLinkHost<'_> {
    fn is_registered(&self) -> Result<bool, String> {
        use tauri_plugin_deep_link::DeepLinkExt;
        self.app
            .deep_link()
            .is_registered(MAILTO)
            .map_err(|err| err.to_string())
    }

    fn register(&self) -> Result<(), String> {
        use tauri_plugin_deep_link::DeepLinkExt;
        self.app
            .deep_link()
            .register(MAILTO)
            .map_err(|err| err.to_string())
    }
}

/// The typed answer to the default-mail ask, as the surface reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RegisterDefaultMailOutcome {
    /// The registration landed: this app is the OS's mailto handler.
    Registered,
    /// The registration already stood: no OS write was made.
    AlreadyDefault,
    /// The registration refused; the sentence names why, ready to show.
    Failed {
        /// The refusal's own sentence.
        reason: String,
    },
}

/// Registers the app as the OS's default mail handler, idempotently:
/// a registration that already stands answers `AlreadyDefault`
/// without touching the OS, because the person may have re-pointed
/// the default at another app in between, and only their own ask
/// moves it back. The landed registration records the first product
/// event of its own shape.
pub(crate) fn register_default_mail_with(
    state: &AppState,
    host: &dyn DefaultMailHost,
) -> RegisterDefaultMailOutcome {
    match host.is_registered() {
        Ok(true) => RegisterDefaultMailOutcome::AlreadyDefault,
        Ok(false) => match host.register() {
            Ok(()) => {
                log::info!("default-mail: the mailto association was registered");
                state
                    .app
                    .record_analytics(&astra_app::AnalyticsEvent::of("default_mail_registered"));
                RegisterDefaultMailOutcome::Registered
            }
            Err(reason) => RegisterDefaultMailOutcome::Failed { reason },
        },
        Err(reason) => RegisterDefaultMailOutcome::Failed { reason },
    }
}

/// Registers the app as the OS's default mail handler: the settings
/// surface's own ask on the offer's acceptance. The registration
/// writes the OS's registry, so the `async` marker keeps the handler
/// off the main thread.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub(crate) fn register_default_mail(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> RegisterDefaultMailOutcome {
    register_default_mail_with(&state, &DeepLinkHost::new(&app))
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
