// SPDX-License-Identifier: GPL-3.0-only
//! The attachment browser's pull: the cross-message browse the modal opens
//! on, answered as the flattened rows beside the coverage report. A child of
//! [`super`], whose command registration this module continues, under the
//! same split the other command halves made for the line cap. The pull takes
//! the two filter legs the surface holds and hands the core's bounded
//! fan-out; Save As rides the existing attachment command verbatim, a browser
//! row carrying the account, key, and part id it names.

use astra_viewmodel::{AttachmentBrowserFilter, AttachmentBrowserView};
use tauri::State;

use crate::AppState;

/// Normalises one filter leg: a trimmed non-empty value crosses, an absent or
/// blank one narrows nothing (never a match-nothing string the core would
/// filter every row out under).
fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Browses attachments across every account under the filter: the core's
/// bounded fan-out, answered as the rows beside the coverage report.
pub(crate) async fn attachment_browser_of(
    state: &AppState,
    sender: Option<String>,
    mime_prefix: Option<String>,
) -> AttachmentBrowserView {
    let filter = AttachmentBrowserFilter {
        sender: clean(sender),
        mime_prefix: clean(mime_prefix),
    };
    state.app.attachment_browser(filter).await
}

/// Browses attachments across mail: the modal's pull, the filter legs the
/// surface holds crossing as the core's filter. The `async` marker keeps the
/// handler off the main thread, because the sweep fetches missing sources
/// over the network.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub(crate) fn attachment_browser(
    state: State<'_, AppState>,
    sender: Option<String>,
    mime_prefix: Option<String>,
) -> AttachmentBrowserView {
    state
        .runtime
        .block_on(|| attachment_browser_of(&state, sender, mime_prefix))
}

#[cfg(test)]
#[path = "attachment_browser_tests.rs"]
mod tests;
