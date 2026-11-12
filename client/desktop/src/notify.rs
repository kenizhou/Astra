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
use tauri::{Emitter, Manager};

/// How many messages one pass may land before the bridge folds them into a
/// single summary notification.
pub(crate) const SUMMARY_AFTER: usize = 3;

/// The event the notified batch crosses to the web on: the notification
/// history's recording channel, the name the web's listener subscribes to.
pub(crate) const NEW_MAIL_EVENT: &str = "new-mail";

/// Where an OS notification goes: the notification plugin in the running
/// app, a recorder in a test.
pub(crate) trait OsNotifier: Send + Sync {
    /// Shows one notification with `title` and `body`.
    fn show(&self, title: &str, body: &str);
}

/// Where the notified batch crosses to the web history: the running app's
/// event channel, a recorder in a test. An error is the channel's own
/// failure; the bridge logs it and never lets it suppress the OS
/// notification that already crossed.
pub(crate) trait HistoryEmitter: Send + Sync {
    /// Emits one notified batch on the history channel.
    fn emit(&self, batch: &NewMailBatch) -> Result<(), String>;
}

/// Reads whether the main window is focused: the suppression the rules
/// rest on, probed at delivery time for the batch in hand.
pub(crate) type FocusProbe = Arc<dyn Fn() -> bool + Send + Sync>;

/// The bridge: one OS sink and one focus probe, driving the rules over
/// whatever batch a pass drained, plus the history channel the notified
/// batch crosses to the web on (the windowed host only).
pub(crate) struct NotifyBridge {
    /// Where the notifications cross.
    sink: Box<dyn OsNotifier>,
    /// Whether the main window is focused right now.
    focused: FocusProbe,
    /// Where the notified batch crosses to the web history; the headless
    /// host holds none.
    history: Option<Box<dyn HistoryEmitter>>,
}

impl NotifyBridge {
    /// A bridge over `sink`, answering focus from `focused`, carrying no
    /// history channel until one is attached.
    pub(crate) fn new(sink: Box<dyn OsNotifier>, focused: FocusProbe) -> Self {
        Self {
            sink,
            focused,
            history: None,
        }
    }

    /// Attaches the history channel the notified batch crosses on.
    pub(crate) fn with_history(mut self, history: Box<dyn HistoryEmitter>) -> Self {
        self.history = Some(history);
        self
    }

    /// The windowed host's bridge: the notification plugin over the
    /// shell's handle, focus read off the main window, and the notified
    /// batch crossing to the web on the new-mail event.
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
        Self::new(
            Box::new(TauriNotifier {
                handle: handle.clone(),
            }),
            focus,
        )
        .with_history(Box::new(TauriHistoryEmitter { handle }))
    }

    /// The headless host's bridge: notifications land in the log, so the
    /// slice harness observes what would have crossed.
    pub(crate) fn logged() -> Self {
        let focus: FocusProbe = Arc::new(|| false);
        Self::new(Box::new(LogNotifier), focus)
    }

    /// Delivers one drained batch under the rules: nothing while the
    /// switch is off or the window is focused, one notification per
    /// message up to [`SUMMARY_AFTER`], and a single summary past it. The
    /// notified batch then crosses to the web history; a channel failure
    /// is logged, never raised, so it cannot suppress the OS notification
    /// that already crossed.
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
        } else {
            for message in &batch.messages {
                self.sink.show(&title(message), &body(message));
            }
        }
        if let Some(history) = &self.history {
            if let Err(err) = history.emit(batch) {
                log::warn!("notifications: the new-mail history event could not be emitted: {err}");
            }
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

/// The running app's history channel: the notified batch crosses to the
/// web on the new-mail event, the share and mailto channels' sibling.
struct TauriHistoryEmitter {
    /// The handle the event answers on.
    handle: tauri::AppHandle,
}

impl HistoryEmitter for TauriHistoryEmitter {
    fn emit(&self, batch: &NewMailBatch) -> Result<(), String> {
        self.handle
            .emit(NEW_MAIL_EVENT, batch)
            .map_err(|err| err.to_string())
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
