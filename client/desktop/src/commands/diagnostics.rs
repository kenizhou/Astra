// SPDX-License-Identifier: GPL-3.0-only
//! The diagnostics half of the command surface: the log tail the
//! Diagnostics viewer renders and the log export over the SP2p save
//! seam. The tail answers the current file's last lines (never a path
//! beyond the lines themselves); the export asks the host's save
//! dialog over the whole current file and answers the typed outcome
//! family the attachment save answers, recording its own product
//! event when the bytes land.
//!
//! A child of [`super`], whose command registration this module
//! continues, under the same split the other command halves made for
//! the line cap.

use tauri::State;

use super::attachments::{HostSave, SaveAttachmentOutcome, SaveHost};
use crate::AppState;

/// The most lines one tail answers: the viewer asks for a screenful,
/// and the cap keeps a runaway ask from hauling the whole file across
/// the IPC.
const TAIL_MAX: usize = 1000;

/// The last `n` lines of the current log file, oldest of the kept
/// first. A file that does not stand answers nothing, and an ask past
/// the cap answers the cap's worth. The answer is the lines
/// themselves: no path crosses beyond them.
pub(crate) fn log_tail_of(state: &AppState, n: usize) -> Vec<String> {
    let n = n.min(TAIL_MAX);
    let Ok(body) = std::fs::read_to_string(&state.log_path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = body.lines().collect();
    let kept = lines.len().saturating_sub(n);
    lines[kept..]
        .iter()
        .map(|line| (*line).to_owned())
        .collect()
}

/// Exports the current log file: the save dialog over the file's own
/// name, the write of its whole bytes, and the typed outcome the
/// surface shows. The event the export records carries the byte count
/// only, the counts-and-shapes rule.
pub(crate) fn log_export_of(state: &AppState, host: &dyn SaveHost) -> SaveAttachmentOutcome {
    let bytes = match std::fs::read(&state.log_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            log::warn!("log-export: the current log could not be read: {err}");
            return SaveAttachmentOutcome::Failed {
                reason: err.to_string(),
            };
        }
    };
    let Some(path) = host.ask_save_path(crate::logging::LOG_FILE) else {
        return SaveAttachmentOutcome::Cancelled;
    };
    match host.write(&path, &bytes) {
        Ok(()) => {
            log::info!("log-export: {} byte(s) exported", bytes.len());
            state.app.record_analytics(
                &astra_app::AnalyticsEvent::of("log_exported")
                    .with_count("bytes", bytes.len() as u64),
            );
            SaveAttachmentOutcome::Saved {
                path: path.display().to_string(),
            }
        }
        Err(err) => {
            log::warn!("log-export: the write refused: {err}");
            SaveAttachmentOutcome::Failed { reason: err }
        }
    }
}

/// Answers the log tail for the Diagnostics viewer. The read blocks
/// on the file, so the `async` marker keeps the handler off the main
/// thread.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub(crate) fn log_tail(state: State<'_, AppState>, n: usize) -> Vec<String> {
    log_tail_of(&state, n)
}

/// Exports the log through the save dialog. The dialog blocks until
/// the reader answers, so the `async` marker keeps the handler off
/// the main thread.
// Tauri's command macro hands `State` by value; the wrapper reads and writes through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub(crate) fn log_export(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> SaveAttachmentOutcome {
    let host = HostSave::new(&app);
    log_export_of(&state, &host)
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
