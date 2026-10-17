// SPDX-License-Identifier: GPL-3.0-only
//! The attachments half of the command surface: the reading pane's Save As.
//! One command reads the chosen attachment's decoded bytes through the app
//! core, suggests the row's file name, asks the host's save dialog where to
//! put them, writes the file, and answers the typed outcome the surface
//! shows: the saved path, the reader's own cancel, or the refusal's
//! sentence. The dialog and the write sit behind the [`SaveHost`] seam, so
//! the command's tests drive both without a real dialog or disk.
//!
//! A child of [`super`], whose command registration this module continues,
//! under the same split the other command halves made for the line cap.

use std::path::{Path, PathBuf};

use engine_api::{AccountId, ProviderKey};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// The typed answer to one Save As, as the surface reads it: the saved
/// path to show, the reader's own cancel, or the refusal's sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SaveAttachmentOutcome {
    /// The bytes landed at the path the reader chose; the path to show.
    Saved {
        /// The path the save dialog answered, as the host spells it.
        path: String,
    },
    /// The reader closed the dialog without choosing; nothing was written.
    Cancelled,
    /// The save could not happen; the sentence names why, ready to show.
    Failed {
        /// The refusal's own sentence: the read's, or the write's.
        reason: String,
    },
}

/// The host leg's seam: the save dialog and the file write, the two host
/// services the save needs, so the tests drive the command without a real
/// dialog or disk.
pub(crate) trait SaveHost {
    /// Asks the reader where to save, suggesting `file_name`; `None` is
    /// the reader's own cancel.
    fn ask_save_path(&self, file_name: &str) -> Option<PathBuf>;
    /// Writes the bytes to the chosen path; the sentence names why not.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String>;
}

/// The production host: the dialog plugin's save dialog beside a plain
/// file write.
struct HostSave<'a> {
    /// The shell handle the dialog plugin answers through.
    app: &'a tauri::AppHandle,
}

impl SaveHost for HostSave<'_> {
    fn ask_save_path(&self, file_name: &str) -> Option<PathBuf> {
        use tauri_plugin_dialog::DialogExt;
        self.app
            .dialog()
            .file()
            .set_file_name(file_name)
            .blocking_save_file()
            .and_then(|chosen| chosen.into_path().ok())
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(path, bytes).map_err(|err| err.to_string())
    }
}

/// Saves one attachment of one message: reads the decoded bytes through the
/// app core, asks the host where to put them with the row's file name as
/// the suggestion, writes them, and answers the typed outcome. Nothing
/// about the read is logged beyond the path the reader chose: the account,
/// the message key, and the suggested name stay out of every log line.
pub(crate) async fn save_attachment_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
    attachment: u32,
    host: &dyn SaveHost,
) -> SaveAttachmentOutcome {
    let content = match state.app.attachment_content(account, key, attachment).await {
        Ok(Some(content)) => content,
        Ok(None) => {
            log::warn!("save-attachment: no attachment of the message matched the id");
            return SaveAttachmentOutcome::Failed {
                reason: "no attachment of the message matched the id".to_owned(),
            };
        }
        Err(err) => {
            log::warn!("save-attachment: the read refused: {err}");
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
            log::info!("save-attachment: saved to {}", path.display());
            SaveAttachmentOutcome::Saved {
                path: path.display().to_string(),
            }
        }
        Err(err) => {
            log::warn!("save-attachment: the write refused: {err}");
            SaveAttachmentOutcome::Failed { reason: err }
        }
    }
}

/// Saves one attachment: the reading pane's Save As over the open message's
/// attachment row. Every failure is the typed outcome the surface shows,
/// never a rejected call. The `async` marker keeps this handler off the
/// main thread, because the save dialog blocks until the reader answers.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub(crate) fn save_attachment(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    account: AccountId,
    key: ProviderKey,
    attachment: u32,
) -> SaveAttachmentOutcome {
    let host = HostSave { app: &app };
    state
        .runtime
        .block_on(|| save_attachment_of(&state, &account, &key, attachment, &host))
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
