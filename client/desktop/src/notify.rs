// SPDX-License-Identifier: GPL-3.0-only
//! The new-mail notification bridge: the drain side of the core's
//! new-mail queue, where the policy D2 states lives. One OS notification
//! per message while the main window is unfocused, a single summary when
//! more than three arrived in a pass, none while the window is focused,
//! and none at all when the switch is off. The core answers what landed;
//! only the host knows the window state and owns the notification plugin,
//! so the rules sit here beside the observer bridge rather than in the
//! core.

use std::sync::Arc;

use astra_app::NewMailBatch;
use astra_app::NewMailNotice;
use tauri::Manager;

/// How many messages one pass may land before the bridge folds them into a
/// single summary notification.
pub(crate) const SUMMARY_AFTER: usize = 3;

/// Where an OS notification goes: the notification plugin in the running
/// app, a recorder in a test.
pub(crate) trait OsNotifier: Send + Sync {
    /// Shows one notification with `title` and `body`.
    fn show(&self, title: &str, body: &str);
}

/// Reads whether the main window is focused: the suppression the rules
/// rest on, probed at delivery time for the batch in hand.
pub(crate) type FocusProbe = Arc<dyn Fn() -> bool + Send + Sync>;

/// The bridge: one OS sink and one focus probe, driving the rules over
/// whatever batch a pass drained.
pub(crate) struct NotifyBridge {
    /// Where the notifications cross.
    sink: Box<dyn OsNotifier>,
    /// Whether the main window is focused right now.
    focused: FocusProbe,
}

impl NotifyBridge {
    /// A bridge over `sink`, answering focus from `focused`.
    pub(crate) fn new(sink: Box<dyn OsNotifier>, focused: FocusProbe) -> Self {
        Self { sink, focused }
    }

    /// The windowed host's bridge: the notification plugin over the
    /// shell's handle, focus read off the main window.
    pub(crate) fn windowed(handle: tauri::AppHandle) -> Self {
        let focus: FocusProbe = {
            let handle = handle.clone();
            Arc::new(move || {
                handle
                    .get_webview_window("main")
                    .and_then(|window| window.is_focused().ok())
                    .unwrap_or(false)
            })
        };
        Self::new(Box::new(TauriNotifier { handle }), focus)
    }

    /// The headless host's bridge: notifications land in the log, so the
    /// slice harness observes what would have crossed.
    pub(crate) fn logged() -> Self {
        let focus: FocusProbe = Arc::new(|| false);
        Self::new(Box::new(LogNotifier), focus)
    }

    /// Delivers one drained batch under the rules: nothing while the
    /// switch is off or the window is focused, one notification per
    /// message up to [`SUMMARY_AFTER`], and a single summary past it.
    pub(crate) fn deliver(&self, batch: &NewMailBatch, enabled: bool) {
        if !enabled || (self.focused)() {
            return;
        }
        let total = batch.messages.len() + batch.overflow;
        if total == 0 {
            return;
        }
        if total > SUMMARY_AFTER {
            self.sink.show("New mail", &summary_body(total));
            return;
        }
        for message in &batch.messages {
            self.sink.show(&title(message), &body(message));
        }
    }
}

impl core::fmt::Debug for NotifyBridge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NotifyBridge").finish_non_exhaustive()
    }
}

/// The running app's sink: the notification plugin the shell registered.
struct TauriNotifier {
    /// The handle the plugin answers on.
    handle: tauri::AppHandle,
}

impl OsNotifier for TauriNotifier {
    fn show(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        if let Err(err) = self
            .handle
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            log::warn!("notifications: a new-mail notification could not be shown: {err}");
        }
    }
}

/// The headless sink: one log line per notification.
struct LogNotifier;

impl OsNotifier for LogNotifier {
    fn show(&self, title: &str, body: &str) {
        log::info!("notification: {title}: {body}");
    }
}

/// One message's notification title: who it is from, the display name the
/// envelope carried or the bare address, and the generic word when it
/// carried neither.
fn title(message: &NewMailNotice) -> String {
    message
        .from
        .as_ref()
        .and_then(|from| {
            from.name
                .as_ref()
                .filter(|name| !name.trim().is_empty())
                .cloned()
                .or_else(|| Some(from.email.clone()))
        })
        .unwrap_or_else(|| "New mail".to_owned())
}

/// One message's notification body: what it is about and how it begins,
/// the parts the envelope carried joined by a newline.
fn body(message: &NewMailNotice) -> String {
    let mut lines = Vec::new();
    if let Some(subject) = message.subject.as_ref().filter(|s| !s.trim().is_empty()) {
        lines.push(subject.clone());
    }
    if let Some(preview) = message.preview.as_ref().filter(|p| !p.trim().is_empty()) {
        lines.push(preview.clone());
    }
    if lines.is_empty() {
        "You have a new message".to_owned()
    } else {
        lines.join("\n")
    }
}

/// The summary's body: how many messages the pass landed, the overflow the
/// queue folded included.
fn summary_body(count: usize) -> String {
    format!("{count} new messages")
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod tests;
