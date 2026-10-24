// SPDX-License-Identifier: GPL-3.0-only
//! The message-list writes: mark-read, flag, and the archive and trash moves,
//! the state changes a list row drives. Ported from mail-calendar's mail ops
//! in their single-row shapes.
//!
//! Every edit goes through the engine's outbox ([`engine_api::Engine::edit_mail`]): it
//! is queued durably first, which shows the keyword change in the store from that
//! moment, then sent to the provider. The outcome is deliberately not surfaced as a
//! return value on the intent path: the mutation result is discarded and the refreshed
//! snapshot publishes what actually holds (the flipped row state once queued; the
//! taken-back state only when the server refuses the edit for good). The method forms
//! answer a typed [`MailWriteError`] so the agent port can report the failures the
//! interactive path collapses into its silent skip.

use engine_api::{MailEdit, Mailbox, MailboxId, MailboxRole, Provider, ProviderKey};
use thiserror::Error;

use crate::{
    reference::{RowRef, ThreadRef},
    App,
};

/// Why a message write could not be applied. The three failures the
/// interactive path folds into a logged skip; the agent port reports
/// them, because an assistant that cannot see a silent no-op will
/// claim success over mail that never moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MailWriteError {
    /// The row's account is no longer configured.
    #[error("the row's account is no longer configured")]
    UnknownAccount,
    /// The row's account has no provider to write through.
    #[error("the row's account has no provider to write through")]
    NoProvider,
    /// The account has no folder of the role the move targets.
    #[error("that account has no folder to move the message to")]
    NoTargetFolder,
    /// A move-to-folder batch named rows and a destination from different
    /// accounts; a move stays within one account, the row's own constraint.
    #[error("a move to a folder stays within one account")]
    CrossAccount,
}

impl<P: Provider> App<P> {
    /// Marks one row read or unread on its owning account. A conversation row marks
    /// every message on the thread.
    ///
    /// # Errors
    ///
    /// [`MailWriteError::UnknownAccount`] or [`MailWriteError::NoProvider`]
    /// when the row's account can no longer be written to.
    pub async fn mark_read(&self, row: RowRef, read: bool) -> Result<(), MailWriteError> {
        self.edit_row_keywords(row, |key| MailEdit::mark_seen(key, read))
            .await
    }

    /// Flags or unflags one row on its owning account, a conversation row's every
    /// member included.
    ///
    /// # Errors
    ///
    /// [`MailWriteError::UnknownAccount`] or [`MailWriteError::NoProvider`]
    /// when the row's account can no longer be written to.
    pub async fn set_flagged(&self, row: RowRef, flagged: bool) -> Result<(), MailWriteError> {
        self.edit_row_keywords(row, |key| MailEdit::set_flagged(key, flagged))
            .await
    }

    /// Moves one row out of the inbox and into the account's archive: a message,
    /// or a whole conversation's every member. The archive destination is
    /// resolved per the shared target resolver; a move the store learns of at the
    /// next sync, so the republished list tells the user when it lands.
    ///
    /// # Errors
    ///
    /// [`MailWriteError::NoTargetFolder`] when the account has no archive to
    /// move to; the account and provider errors of [`App::mark_read`].
    pub async fn archive_message(&self, row: RowRef) -> Result<(), MailWriteError> {
        self.move_row_to(row, &MailboxRole::Archive).await
    }

    /// Moves one row to the account's trash, recoverable from there: a message,
    /// or a whole conversation's every member. The same move edit
    /// [`App::archive_message`] issues, aimed at the trash; nothing is
    /// permanently deleted.
    ///
    /// # Errors
    ///
    /// [`MailWriteError::NoTargetFolder`] when the account has no trash to
    /// move to; the account and provider errors of [`App::mark_read`].
    pub async fn move_to_trash(&self, row: RowRef) -> Result<(), MailWriteError> {
        self.move_row_to(row, &MailboxRole::Trash).await
    }

    /// Moves a set of messages to one folder of their own account: one move
    /// edit per row, the same outbox discipline the single-row moves ride. A
    /// row already sitting in the destination is a no-op answered without
    /// queueing anything.
    ///
    /// # Errors
    ///
    /// [`MailWriteError::CrossAccount`] when the rows and the destination
    /// folder name different accounts; the account and provider errors of
    /// [`App::mark_read`].
    pub async fn move_messages(
        &self,
        rows: Vec<crate::reference::MessageRef>,
        folder: crate::reference::FolderRef,
    ) -> Result<(), MailWriteError> {
        if rows.iter().any(|row| row.account != folder.account) {
            log::warn!("move: a move to a folder stays within one account");
            return Err(MailWriteError::CrossAccount);
        }
        let account = folder.account.clone();
        // The account handle is cloned out so the guard is released before the
        // provider round-trip; an account the app no longer holds, or one
        // whose provider is gone, cannot be written to.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("move: the row's account is no longer configured");
            return Err(MailWriteError::UnknownAccount);
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("move: the row's account has no provider to write through");
            return Err(MailWriteError::NoProvider);
        };
        // The destination must name a folder of the account's own tree: a
        // well-formed key naming nothing would queue one refused edit per
        // row, the silent no-op the typed forms exist to prevent.
        let tree = self.engine.mailboxes(&account).await.unwrap_or_default();
        if !tree.iter().any(|mailbox| mailbox.id.as_str() == folder.key) {
            log::warn!("move: the destination key names no folder of the account");
            return Err(MailWriteError::NoTargetFolder);
        }
        let Ok(destination) = MailboxId::try_from(folder.key.as_str()) else {
            log::warn!("move: the destination key names no folder");
            return Err(MailWriteError::NoTargetFolder);
        };
        let mut queued = false;
        for row in rows {
            // A row already sitting in the destination is a no-op: answered
            // without queueing anything, the whole batch when every row is.
            let filed_there = crate::reading::resolve(&self.engine, &row)
                .await
                .is_some_and(|stored| stored.mailboxes.contains(&destination));
            if filed_there {
                continue;
            }
            let edit = MailEdit::move_to(row.key.clone(), destination.clone());
            // Discard-and-resync: whether the server took it or the op waits
            // in the outbox, the snapshot below is what tells the user.
            if let Err(err) = self
                .engine
                .edit_mail(provider, &account, &generated_idempotency(), &edit)
                .await
            {
                log::warn!("move: the mail edit failed: {err}");
            }
            queued = true;
        }
        if !queued {
            return Ok(());
        }
        // The write changed what the list shows, so an in-flight rebuild for the older
        // state must not win by finishing last.
        self.bump_generation();
        self.invalidate_list_cache();
        self.rebuild_snapshot().await;
        Ok(())
    }

    /// Moves one row's every member key to the account's mailbox for `role`
    /// through the engine's outbox, then republishes the list: the same
    /// discard-and-resync discipline the keyword writes ride, over the move
    /// edit instead.
    async fn move_row_to(&self, row: RowRef, role: &MailboxRole) -> Result<(), MailWriteError> {
        let account = row.account().clone();
        let keys = self.row_member_keys(&row).await;
        if keys.is_empty() {
            return Ok(());
        }
        // The account handle is cloned out so the guard is released before the
        // provider round-trip; an account the app no longer holds, or one
        // whose provider is gone, cannot be written to.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("move: the row's account is no longer configured");
            return Err(MailWriteError::UnknownAccount);
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("move: the row's account has no provider to write through");
            return Err(MailWriteError::NoProvider);
        };
        let tree = self.engine.mailboxes(&account).await.unwrap_or_default();
        let Some(destination) = resolve_move_target(&tree, role) else {
            log::warn!("move: the account has no {role:?} folder to move to");
            return Err(MailWriteError::NoTargetFolder);
        };
        for key in keys {
            let edit = MailEdit::move_to(key, destination.clone());
            if let Err(err) = self
                .engine
                .edit_mail(provider, &account, &generated_idempotency(), &edit)
                .await
            {
                log::warn!("move: the mail edit failed: {err}");
            }
        }
        // The write changed what the list shows, so an in-flight rebuild for the older
        // state must not win by finishing last.
        self.bump_generation();
        self.invalidate_list_cache();
        self.rebuild_snapshot().await;
        Ok(())
    }

    /// Resolves `row` to its member keys, applies one keyword `edit` per key through the
    /// engine's outbox, then republishes the list past the row cache: the store shows a
    /// queued keyword change at once, so the rebuild carries the flipped row state
    /// before any server answer, and a refusal for good is what shows instead.
    async fn edit_row_keywords(
        &self,
        row: RowRef,
        build: impl Fn(ProviderKey) -> MailEdit,
    ) -> Result<(), MailWriteError> {
        let account = row.account().clone();
        let keys = self.row_member_keys(&row).await;
        if keys.is_empty() {
            return Ok(());
        }
        // The account handle is cloned out so the guard is released before the provider
        // round-trip; an account the app no longer holds, or one whose provider is
        // gone, cannot be written and is reported rather than guessed at.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("edit: the row's account is no longer configured");
            return Err(MailWriteError::UnknownAccount);
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("edit: the row's account has no provider to write through");
            return Err(MailWriteError::NoProvider);
        };
        for key in keys {
            let edit = build(key);
            // Discard-and-resync: whether the server took it or the op waits in the
            // outbox, the snapshot below is what tells the user.
            if let Err(err) = self
                .engine
                .edit_mail(provider, &account, &generated_idempotency(), &edit)
                .await
            {
                log::warn!("edit: the mail edit failed: {err}");
            }
        }
        // The write changed what the list shows, so an in-flight rebuild for the older
        // state must not win by finishing last.
        self.bump_generation();
        self.invalidate_list_cache();
        self.rebuild_snapshot().await;
        Ok(())
    }

    /// Every message key a row stands for: one for a message row, the thread
    /// index's members for a conversation row, so a conversation shown in a
    /// windowed list still resolves the members the window cut off.
    ///
    /// A message the server never threaded projects under its own provider key as the
    /// thread id (the list's grouping convention), so the store holds no thread for it
    /// and the index read finds nothing; that lone message is resolved by key instead.
    pub(crate) async fn row_member_keys(&self, row: &RowRef) -> Vec<ProviderKey> {
        match row {
            RowRef::Message(message) => vec![message.key.clone()],
            RowRef::Thread(thread) => self.thread_member_keys(thread).await,
        }
    }

    /// Every message key a thread row stands for, straight from the store's thread
    /// index.
    async fn thread_member_keys(&self, thread: &ThreadRef) -> Vec<ProviderKey> {
        let members = self
            .engine
            .mail_on_threads(
                std::slice::from_ref(&thread.account),
                [thread.thread_id.as_str()],
            )
            .await
            .unwrap_or_default();
        if !members.is_empty() {
            return members.into_iter().map(|member| member.mail.key).collect();
        }
        ProviderKey::new(thread.thread_id.clone())
            .map(|key| vec![key])
            .unwrap_or_default()
    }
}

/// The mailbox a move to `role` lands in: the RFC 6154 SPECIAL-USE role,
/// else a conventional folder name, else; for Archive only, the account's
/// RFC 6154 `\All` mailbox. `None` when none resolves.
///
/// The `\All` fallback exists because Gmail has no Archive folder in any
/// form: archiving there is the absence of the Inbox label, and the
/// archived message's home is All Mail, which the engine's adapter surfaces
/// as a synthetic `\All` mailbox. Both earlier lookups miss, so without this
/// the move is never built and archive is a silent no-op on every Gmail
/// account. It is deliberately last: a server that tags a real `\Archive`
/// folder, or names one conventionally, still wins, so this can only turn a
/// no-op into a move, never redirect an existing one.
pub(crate) fn resolve_move_target(tree: &[Mailbox], role: &MailboxRole) -> Option<MailboxId> {
    tree.iter()
        .find(|mailbox| mailbox.role.as_ref() == Some(role))
        .or_else(|| {
            tree.iter()
                .find(|mailbox| name_matches_role(&mailbox.name, role))
        })
        .or_else(|| {
            (role == &MailboxRole::Archive)
                .then(|| {
                    tree.iter()
                        .find(|mailbox| mailbox.role.as_ref() == Some(&MailboxRole::All))
                })
                .flatten()
        })
        .map(|mailbox| mailbox.id.clone())
}

/// Whether `name` is a conventional folder name for `role`: the fallback
/// used when a server does not advertise the SPECIAL-USE role (common for
/// Archive). Matches the leaf of a hierarchical name (for example
/// `INBOX.Archive`) case-insensitively. Only the move targets are matched,
/// and only after the role lookup misses, so it can never override a
/// server-tagged folder.
fn name_matches_role(name: &str, role: &MailboxRole) -> bool {
    let leaf = name.rsplit(['/', '.', '\\']).next().unwrap_or(name).trim();
    let candidates: &[&str] = match role {
        MailboxRole::Archive => &[
            "archive",
            "archives",
            "archief",
            "archieven",
            "archiv",
            "archivio",
            "archivo",
            "arkiv",
        ],
        MailboxRole::Trash => &[
            "trash",
            "deleted",
            "deleted items",
            "deleted messages",
            "bin",
            "prullenbak",
            "papierkorb",
            "corbeille",
            "cestino",
            "papelera",
        ],
        MailboxRole::Junk => &[
            "junk",
            "junk mail",
            "junk e-mail",
            "spam",
            "bulk mail",
            "ongewenste e-mail",
        ],
        _ => return false,
    };
    candidates
        .iter()
        .any(|candidate| leaf.eq_ignore_ascii_case(candidate))
}

/// A unique-per-intent idempotency key for an edit, from the shared mint
/// under the family's own prefix. Unique per call rather than per target, so
/// mark-read then mark-unread of one message never collapses into a single op.
fn generated_idempotency() -> String {
    crate::mint_idempotency("edit")
}

#[cfg(test)]
#[path = "mail_writes_tests.rs"]
mod tests;
