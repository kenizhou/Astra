// SPDX-License-Identifier: GPL-3.0-only
//! The `.eml` half of the command surface: the reading pane's Save as
//! `.eml`. One command reads the message's raw source bytes through the app
//! core (the engine's own source read, never a re-serialization), suggests
//! the file name the core derives from the subject, asks the host's save
//! dialog where to put them, writes the file, and answers the typed outcome
//! family the attachment save answers: the saved path, the reader's own
//! cancel, or the refusal's sentence. The dialog and the write sit behind
//! the [`SaveHost`] seam the attachment save owns, so this command's tests
//! drive both without a real dialog or disk.
//!
//! A child of [`super`], under the same split the other command halves made
//! for the line cap.

use engine_api::{AccountId, ProviderKey};
use tauri::State;

use super::attachments::{HostSave, SaveAttachmentOutcome, SaveHost};
use crate::AppState;

/// Saves one message as an `.eml`: reads the engine's source bytes through
/// the app core, asks the host where to put them with the subject's
/// sanitised name as the suggestion, writes them, and answers the typed
/// outcome. Nothing about the read is logged beyond the path the reader
/// chose: the account, the message key, and the suggested name stay out of
/// every log line.
pub(crate) async fn eml_save_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
    host: &dyn SaveHost,
) -> SaveAttachmentOutcome {
    let content = match state.app.eml_content(account, key).await {
        Ok(content) => content,
        Err(err) => {
            log::warn!("save-eml: the read refused: {err}");
            return SaveAttachmentOutcome::Failed {
                reason: err.to_string(),
            };
        }
    };
    let Some(path) = host.ask_save_path(&content.file_name) else {
        return SaveAttachmentOutcome::Cancelled;
    };
    match host.write(&path, &content.bytes) {
        Ok(()) => {
            log::info!("save-eml: saved to {}", path.display());
            SaveAttachmentOutcome::Saved {
                path: path.display().to_string(),
            }
        }
        Err(err) => {
            log::warn!("save-eml: the write refused: {err}");
            SaveAttachmentOutcome::Failed { reason: err }
        }
    }
}

/// Saves one message as an `.eml`: the reading pane's Save as .eml over the
/// open message. Every failure is the typed outcome the surface shows,
/// never a rejected call. The `async` marker keeps this handler off the
/// main thread, because the save dialog blocks until the reader answers.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub(crate) fn eml_save(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    account: AccountId,
    key: ProviderKey,
) -> SaveAttachmentOutcome {
    let host = HostSave::new(&app);
    state
        .runtime
        .block_on(|| eml_save_of(&state, &account, &key, &host))
}

#[cfg(test)]
#[path = "eml_tests.rs"]
mod tests;
