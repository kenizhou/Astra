// SPDX-License-Identifier: GPL-3.0-only
//! The drafts half of the command surface: the pull for the draft-status
//! snapshot, and the read that opens the account's latest stored draft for a
//! composer to resume. A child of [`super`], whose command registration this
//! module continues.
//!
//! Split from the command module itself to keep each file under the 500-line
//! limit, the same split the app core's protocol made for its drafts intents.
//! The shape follows the reading pull: thin `#[tauri::command]` wrappers over
//! plain functions the tests drive.

use serde::{Deserialize, Serialize};
use tauri::State;

use astra_app::{CompositionId, DraftResume};
use astra_viewmodel::DraftStatusSnapshot;
use engine_api::AccountId;

use crate::AppState;

/// The stored draft a composer resumes, as the command surface hands it to the
/// webview: the core's [`DraftResume`] at the IPC's own serialisable shape, one
/// flat record the composer hydrates its fields and its editor from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResumeView {
    /// The account whose Drafts folder the draft was in, and which its saves go
    /// back to.
    pub(crate) account: String,
    /// The `To` field, comma-joined.
    pub(crate) to: String,
    /// The `Cc` field, comma-joined.
    pub(crate) cc: String,
    /// The `Bcc` field, comma-joined; only what the stored copy carried.
    pub(crate) bcc: String,
    /// The subject.
    pub(crate) subject: String,
    /// The body as plain text: what a composer opens without a second markup
    /// parser.
    pub(crate) body_text: String,
}

impl From<DraftResume> for ResumeView {
    fn from(resumed: DraftResume) -> Self {
        Self {
            account: resumed.account,
            to: resumed.to,
            cc: resumed.cc,
            bcc: resumed.bcc,
            subject: resumed.subject,
            body_text: resumed.body_text,
        }
    }
}

/// The published draft-status snapshot: the composition whose save moved and how
/// it ended, or `None` when no composition has a save to report.
pub(crate) fn draft_status_of(state: &AppState) -> Option<DraftStatusSnapshot> {
    state.app.draft_status()
}

/// Opens the account's latest stored draft into `composition`, the composition
/// the webview minted for the composer it is about to open: the core adopts the
/// stored copy so the composer's first save supersedes it, and answers the
/// fields to hydrate from. `None` when the account holds no draft or its body
/// cannot be read, in which case nothing was adopted either.
pub(crate) async fn draft_resume_of(
    state: &AppState,
    account: &AccountId,
    composition: CompositionId,
) -> Option<ResumeView> {
    state
        .app
        .resume_latest_draft(account, composition)
        .await
        .map(ResumeView::from)
}

/// Pulls the draft-status snapshot: the composer's quiet hint about its last
/// save, or null when no composition has one. Draft status is signalled on every
/// publish, like reading, so this is pulled per signal rather than on a debounce.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn draft_status(state: State<'_, AppState>) -> Option<DraftStatusSnapshot> {
    draft_status_of(&state)
}

/// Opens the account's latest stored draft into the composition the webview
/// minted, and answers the record to hydrate the composer from, or null when
/// there is no draft to resume. The ids are the wire's own gate: a blank
/// composition id is refused before this body runs. The read runs on the
/// runtime's driver thread, like the add flow's dial, so the command waits for
/// it and the composer learns what to open with.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn draft_resume(
    state: State<'_, AppState>,
    account: AccountId,
    composition: CompositionId,
) -> Option<ResumeView> {
    state
        .runtime
        .block_on(|| draft_resume_of(&state, &account, composition))
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod tests;
