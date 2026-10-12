// SPDX-License-Identifier: GPL-3.0-only
//! The intent dispatch table: one inbound [`Intent`] in, the matching use-case out.
//! Split from the crate root so the runtime's state and lifecycle stay separate from
//! the command surface.

use thiserror::Error;

use engine_api::Provider;

use crate::{App, Intent, Scope};

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
                // The pass may have moved the open message (fresh content) or
                // dropped it from the store (deleted server-side): the pane
                // follows either way rather than showing the body as it was.
                self.republish_reading().await;
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
                // it: a differently grouped list is a different list, and it opens at the
                // top. The shared path answers on the settings surface beside the list,
                // because the grouping is a persisted setting too.
                self.set_view_mode(mode).await;
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
            // The reading pane: one open, one close, both publishing the surface.
            Intent::OpenMessage { message } => {
                self.open_message(message).await;
            }
            Intent::CloseReading => {
                self.close_reading();
            }
            Intent::Folders(_change) => return Err(AppError::NotYet),
            // The people family: the reads position the list and the detail
            // over the engine's own matching, the writes name a card, and
            // every outcome the people snapshot's write states and create
            // targets carry, never a return value.
            Intent::People(change) => {
                self.dispatch_people(change).await;
            }
            // The calendar family: reads position the grids (and widen the
            // engine's expansion horizon to cover the window), writes change
            // the diary, and every outcome the calendar snapshot's write
            // states and prompts carry, never a return value.
            Intent::Calendar(change) => {
                self.dispatch_calendar(change).await;
            }
            // The compose family: the prefill is computed and published, never
            // returned; the composer opens on the surface's snapshot.
            Intent::Compose(change) => {
                self.dispatch_compose(change).await;
            }
            // The composer's send: one verb whose outcome the queue holds and
            // the sending surface publishes, never a return value. The reply
            // and the forward are the same verb threaded, the original's
            // chain re-resolved so each joins the conversation it answers.
            Intent::SubmitRichMail {
                from,
                to,
                cc,
                bcc,
                subject,
                document,
                blobs,
            } => {
                self.submit_rich_mail(from, to, cc, bcc, subject, document, blobs)
                    .await;
            }
            Intent::SubmitRichReply {
                from,
                to,
                cc,
                in_reply_to,
                subject,
                document,
                blobs,
            } => {
                self.submit_rich_reply(from, to, cc, in_reply_to, subject, document, blobs)
                    .await;
            }
            Intent::SubmitRichForward {
                from,
                to,
                subject,
                document,
                blobs,
            } => {
                self.submit_rich_forward(from, to, subject, document, blobs)
                    .await;
            }
            // The outbox family: the same discard-and-resync discipline the
            // list writes follow, over the engine's queue ops.
            Intent::Outbox(change) => {
                self.dispatch_outbox(change).await;
            }
            // The composer's three verbs. Outcomes reach the user through the
            // draft-status surface, never a return value, the same discipline the
            // list writes follow.
            Intent::Drafts(change) => {
                self.dispatch_drafts(change).await;
            }
            // The search family: a query is a mode over the list, not a
            // destination. A blank field clears just as an explicit None does,
            // because empty is navigation, not a search for nothing; clearing
            // also drops the scope, so the next search opens across everything
            // rather than silently inheriting a narrowing the user cannot see.
            Intent::Search { query } => {
                let query = query.filter(|query| !query.trim().is_empty());
                self.set_search(query);
                self.reset_window();
                self.bump_generation();
                self.rebuild_snapshot().await;
            }
            // Narrowing re-runs the live query rather than filtering the
            // published rows, so the depth the state reports stays honest too.
            // Without a live query there is nothing to re-run and nothing
            // stored.
            Intent::SetSearchScope { scope } => {
                if self.narrow_search(scope) {
                    self.reset_window();
                    self.bump_generation();
                    self.rebuild_snapshot().await;
                }
            }
            // The settings family: every verb persists what it moves before
            // the surface answers, and the outcomes (a landed setting, a
            // refusal, an account's dav editor state) reach the user through
            // the settings snapshot, never a return value.
            Intent::Settings(change) => {
                self.dispatch_settings(change).await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
