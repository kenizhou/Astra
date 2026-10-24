// SPDX-License-Identifier: GPL-3.0-only
//! The spam-report writes: the MarkJunk and MarkNotJunk intents' engine path,
//! the outbox-mediated report every transport applies itself. A report is one
//! act, not a keyword edit plus a move: the caller resolves the destination
//! exactly as it resolves Trash for a delete, hands it to the report, and the
//! provider files the message as part of reporting (the engine's contract; the
//! IMAP, JMAP, Graph, and Gmail adapters all do), so this path never queues a
//! move edit beside a report.
//!
//! The refusals follow the family's shape: the account and provider errors the
//! list writes answer, the capability gate the folder writes answer, and the
//! no-folder error the moves answer. The outcome is deliberately not a return
//! value on the intent path: the report rides the durable outbox, and the
//! republished list tells the user what holds.

use engine_api::{MailboxRole, MessageReport, Provider, ReportVerdict};
use thiserror::Error;

use crate::{mail_writes::resolve_move_target, reference::RowRef, App};

/// Why a report could not be applied. The typed refusals the agent port
/// reports; the interactive path folds them into a logged skip, the same
/// discipline the list writes follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReportWriteError {
    /// The row's account is no longer configured.
    #[error("the row's account is no longer configured")]
    UnknownAccount,
    /// The row's account has no provider to report through.
    #[error("the row's account has no provider to report through")]
    NoProvider,
    /// The account's provider cannot report mail, or cannot express the
    /// verdict the verb carries.
    #[error("that account cannot report its mail")]
    NotCapable,
    /// The account has no folder the report files the message into.
    #[error("that account has no folder to file the message into")]
    NoTargetFolder,
}

impl<P: Provider> App<P> {
    /// Reports every message `row` stands for as spam and files it into the
    /// account's Junk folder, one durable report per message.
    ///
    /// # Errors
    ///
    /// [`ReportWriteError::NotCapable`] when the account's provider cannot
    /// report mail; the account, provider, and folder errors of
    /// [`App::mark_not_junk`].
    pub async fn mark_junk(&self, row: RowRef) -> Result<(), ReportWriteError> {
        self.report_row(row, ReportVerdict::Junk).await
    }

    /// Reports every message `row` stands for as legitimate and files it back
    /// into the account's Inbox, one durable report per message.
    ///
    /// # Errors
    ///
    /// Every [`ReportWriteError`], as [`App::mark_junk`] answers it.
    pub async fn mark_not_junk(&self, row: RowRef) -> Result<(), ReportWriteError> {
        self.report_row(row, ReportVerdict::NotJunk).await
    }

    /// Runs one report verb over `row`: resolve the account's capability and
    /// the folder the verdict files into, then hand the provider one durable
    /// report per member message, each with its own idempotency key so a junk
    /// report and a later not-junk correction of the same message stay two
    /// ops. The destination is the account's Junk folder for a junk verdict
    /// and its Inbox for a not-junk one, resolved by the shared move-target
    /// resolver: the RFC 6154 role first and the conventional folder name
    /// otherwise for the junk verdict; the Inbox resolves by its role,
    /// which every family guarantees.
    async fn report_row(
        &self,
        row: RowRef,
        verdict: ReportVerdict,
    ) -> Result<(), ReportWriteError> {
        let account = row.account().clone();
        let keys = self.row_member_keys(&row).await;
        if keys.is_empty() {
            return Ok(());
        }
        // The account handle is cloned out so the guard is released before the
        // provider round-trip; an account the app no longer holds, or one
        // whose provider is gone, cannot be reported through.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("report: the row's account is no longer configured");
            return Err(ReportWriteError::UnknownAccount);
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("report: the row's account has no provider to report through");
            return Err(ReportWriteError::NoProvider);
        };
        let Some(controls) = provider.connection_info().capabilities.mail_report() else {
            log::warn!("report: the account's provider cannot report mail");
            return Err(ReportWriteError::NotCapable);
        };
        if !controls.verdicts.allows(verdict) {
            log::warn!("report: the account's provider cannot report that verdict");
            return Err(ReportWriteError::NotCapable);
        }
        let tree = self.engine.mailboxes(&account).await.unwrap_or_default();
        let role = if verdict.files_as_junk() {
            MailboxRole::Junk
        } else {
            MailboxRole::Inbox
        };
        let Some(destination) = resolve_move_target(&tree, &role) else {
            log::warn!("report: the account has no {role} folder to file into");
            return Err(ReportWriteError::NoTargetFolder);
        };
        for key in keys {
            // Discard-and-resync, per op: whether the provider took the
            // report or the op waits in the outbox, the republished list
            // below is what tells the user.
            let report = MessageReport::new(key, verdict, destination.clone());
            if let Err(err) = self
                .engine
                .report_message(
                    provider,
                    &account,
                    &crate::mint_idempotency("report"),
                    &report,
                )
                .await
            {
                log::warn!("report: the message report failed: {err}");
            }
        }
        // The report files the message, so the list it leaves changes: the
        // same generation and cache discipline the move writes ride.
        self.bump_generation();
        self.invalidate_list_cache();
        self.rebuild_snapshot().await;
        Ok(())
    }
}

#[cfg(test)]
#[path = "report_writes_tests.rs"]
mod tests;
