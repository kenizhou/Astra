// SPDX-License-Identifier: GPL-3.0-only
//! The intent dispatch table: one inbound [`Intent`] in, the matching use-case out.
//! Split from the crate root so the runtime's state and lifecycle stay separate from
//! the command surface.

use thiserror::Error;

use engine_api::Provider;

use crate::{App, Intent, Scope, ViewMode};

/// An error the dispatch loop reports to its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppError {
    /// The intent family is declared in the protocol but not implemented in this
    /// milestone. The caller learns the shape now and the behaviour later, rather than
    /// the protocol growing a breaking variant then.
    #[error("not yet available in this milestone")]
    NotYet,
}

impl<P: Provider> App<P> {
    /// Handles one inbound [`Intent`]. On completion the changed surface is signalled
    /// and the host pulls the new snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotYet`] for an intent family this milestone does not
    /// implement; every implemented arm always returns `Ok`, because a mail mutation's
    /// outcome is told through the surfaces, not a return value.
    pub async fn dispatch(&self, intent: Intent) -> Result<(), AppError> {
        match intent {
            Intent::RefreshMail => {
                self.refresh_mail().await;
            }
            Intent::SelectAccount(account) => {
                // The account's own all-mail view, or the unified list. Either way the
                // selected folder goes with it: a folder belongs to one account, and
                // `Scope` will not hold one without it.
                self.set_scope(Scope::for_account(account));
                self.reset_window();
                self.bump_generation();
                self.rebuild_snapshot().await;
            }
            Intent::SetAccountExpanded { account, expanded } => {
                // No `reset_window`: the list on screen has not changed, only the tree
                // beside it, so scrolling back to the top would be a visible
                // non-sequitur.
                self.set_account_expanded(&account, expanded).await;
            }
            Intent::SetFolderExpanded { folder, expanded } => {
                // The same reasoning one level down. In particular this does **not**
                // select the folder: opening a folder to see what is inside it is not
                // opening its mail.
                self.set_folder_expanded(&folder, expanded).await;
            }
            Intent::SelectFolder { folder } => {
                // One write, so the account and the key can never be half-applied: a
                // reader between two writes is how the same key got resolved against
                // the wrong account. The selection is persisted, so a relaunch
                // reopens it.
                self.set_scope(Scope::Folder(folder));
                self.reset_window();
                self.bump_generation();
                self.rebuild_snapshot().await;
            }
            Intent::SetViewMode(mode) => {
                // Persist the grouping so it survives a restart. The window resets with
                // it: a differently grouped list is a different list, and it opens at
                // the top.
                self.set_view_mode(mode);
                self.reset_window();
                self.bump_generation();
                self.rebuild_snapshot().await;
            }
            Intent::ShowMore => {
                self.grow_window();
                self.bump_generation();
                self.rebuild_snapshot().await;
            }
            // Discard-and-resync writes: the outcome reaches the user through the
            // republished snapshot (the store shows a queued keyword change at once),
            // never through a return value.
            Intent::MarkRead { row, read } => {
                self.mark_read(row, read).await;
            }
            Intent::SetFlagged { row, flagged } => {
                self.set_flagged(row, flagged).await;
            }
            Intent::Folders(_change) => return Err(AppError::NotYet),
        }
        Ok(())
    }

    /// Records the grouping preference, read-modify-write against the file.
    fn set_view_mode(&self, mode: ViewMode) {
        self.pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_view_mode(mode);
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
