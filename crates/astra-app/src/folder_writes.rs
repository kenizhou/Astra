// SPDX-License-Identifier: GPL-3.0-only
//! The folder-tree writes: create, rename, move, trash, and delete, each mapped
//! onto the engine's outbox-mediated `edit_mailbox` with a minted idempotency
//! key, so an edit queued offline lands when the server returns. The account's
//! capability snapshot gates the verbs; a provider without mailbox writes is
//! refused before anything is queued.

use engine_api::{AccountId, Mailbox, MailboxChange, MailboxId, MailboxPlace, MailboxRole};
use thiserror::Error;

use crate::{
    mail_writes::resolve_move_target, protocol::FolderIntent, reference::FolderRef, App, Scope,
};

/// Why a folder write could not be applied. The app-level refusals the
/// interactive path folds into a logged skip; the engine's own refusal of a
/// change (a folder that moved on elsewhere) arrives as its error text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FolderWriteError {
    /// The account is no longer configured.
    #[error("the account is no longer configured")]
    UnknownAccount,
    /// The account has no provider to write through.
    #[error("the account has no provider to write through")]
    NoProvider,
    /// The account's provider cannot change a folder tree.
    #[error("that account cannot change its folders")]
    NotCapable,
    /// The folder key names no folder in the account's tree.
    #[error("that account holds no such folder")]
    UnknownFolder,
    /// The account has no Trash folder to trash into.
    #[error("that account has no folder to trash into")]
    NoTrashFolder,
    /// The engine refused the change; the text is its own.
    #[error("the folder change was refused: {0}")]
    Refused(String),
}

impl<P: engine_api::Provider> App<P> {
    /// Applies one folder-tree change through the engine's outbox, re-selects
    /// the folder under the receipt's key when the edit renamed or moved what
    /// was selected, and republishes the pane over the reconciled tree.
    ///
    /// # Errors
    ///
    /// Every [`FolderWriteError`]: the app-level refusals above, and the
    /// engine's own refusal of the change carried as its text.
    pub(crate) async fn write_folder(&self, change: FolderIntent) -> Result<(), FolderWriteError> {
        let account = account_of(&change).clone();
        // The account handle is cloned out so the guard is released before the
        // provider round-trip; an account the app no longer holds, or one
        // whose provider is gone, cannot be written to.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("folders: the account is no longer configured");
            return Err(FolderWriteError::UnknownAccount);
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("folders: the account has no provider to write through");
            return Err(FolderWriteError::NoProvider);
        };
        if !provider.connection_info().capabilities.mailbox_writes() {
            log::warn!("folders: the account's provider cannot change its folders");
            return Err(FolderWriteError::NotCapable);
        }
        let tree = self.engine.mailboxes(&account).await.unwrap_or_default();
        let (edit, edited_key) = plan_change(&tree, change)?;
        let written = self
            .engine
            .edit_mailbox(provider, &account, &generated_idempotency(), &edit)
            .await
            .map_err(|err| FolderWriteError::Refused(err.to_string()))?;
        self.reselect_after_edit(
            &account,
            edited_key.as_deref(),
            written.outcome.mailbox.as_ref(),
        );
        // The edit changed what the pane draws, so an in-flight rebuild for the
        // older tree must not win by finishing last.
        self.bump_generation();
        self.invalidate_list_cache();
        self.rebuild_snapshot().await;
        Ok(())
    }

    /// Moves the selection onto the key the receipt named for the folder the
    /// edit touched: a rename or a move mints a new key, and a selection left
    /// on the old one would name a folder the tree no longer holds. A folder
    /// removed for good returns the selection to the account's own view.
    fn reselect_after_edit(
        &self,
        account: &AccountId,
        edited_key: Option<&str>,
        receipt: Option<&MailboxId>,
    ) {
        let Some(edited_key) = edited_key else {
            return;
        };
        let selected = self.scope.lock().expect("scope mutex poisoned").clone();
        let Scope::Folder(current) = selected else {
            return;
        };
        if current.account != *account || current.key != edited_key {
            return;
        }
        let moved = receipt.map_or_else(
            || Scope::Account(account.clone()),
            |key| {
                Scope::Folder(FolderRef {
                    account: account.clone(),
                    key: key.as_str().to_owned(),
                })
            },
        );
        self.set_scope(moved);
        self.reset_window();
    }
}

/// The change the engine is asked to apply, beside the key of the folder it
/// touches (`None` for a create): the pair the re-selection reads.
fn plan_change(
    tree: &[Mailbox],
    change: FolderIntent,
) -> Result<(MailboxChange, Option<String>), FolderWriteError> {
    match change {
        FolderIntent::Create { parent, name, .. } => Ok((
            MailboxChange::Create {
                parent: keyed(parent)?,
                name,
            },
            None,
        )),
        FolderIntent::Rename { folder, name, .. } => {
            let mailbox = find(tree, &folder)?;
            Ok((
                MailboxChange::Update {
                    target: mailbox.id.clone(),
                    seen: MailboxPlace::of(mailbox),
                    name,
                    parent: mailbox.parent.clone(),
                },
                Some(folder),
            ))
        }
        FolderIntent::Move { folder, parent, .. } => {
            let mailbox = find(tree, &folder)?;
            Ok((
                MailboxChange::Update {
                    target: mailbox.id.clone(),
                    seen: MailboxPlace::of(mailbox),
                    name: mailbox.name.clone(),
                    parent: keyed(parent)?,
                },
                Some(folder),
            ))
        }
        FolderIntent::Trash { folder, .. } => {
            let mailbox = find(tree, &folder)?;
            let Some(trash) = resolve_move_target(tree, &MailboxRole::Trash) else {
                log::warn!("folders: the account has no Trash folder to trash into");
                return Err(FolderWriteError::NoTrashFolder);
            };
            Ok((
                MailboxChange::Trash {
                    target: mailbox.id.clone(),
                    seen: MailboxPlace::of(mailbox),
                    trash,
                },
                Some(folder),
            ))
        }
        FolderIntent::Delete { folder, .. } => {
            let target = MailboxId::try_from(folder.as_str())
                .map_err(|_| FolderWriteError::UnknownFolder)?;
            Ok((MailboxChange::Delete { target }, Some(folder)))
        }
    }
}

/// The folder `key` names in `tree`, or the typed refusal when it names
/// nothing.
fn find<'a>(tree: &'a [Mailbox], key: &str) -> Result<&'a Mailbox, FolderWriteError> {
    tree.iter()
        .find(|mailbox| mailbox.id.as_str() == key)
        .ok_or(FolderWriteError::UnknownFolder)
}

/// A folder key as an id, `None` passing through; a key that cannot be one
/// names no folder.
fn keyed(key: Option<String>) -> Result<Option<MailboxId>, FolderWriteError> {
    key.map(|key| MailboxId::try_from(key.as_str()))
        .transpose()
        .map_err(|_| FolderWriteError::UnknownFolder)
}

/// The account every variant of a folder intent names.
fn account_of(change: &FolderIntent) -> &AccountId {
    match change {
        FolderIntent::Create { account, .. }
        | FolderIntent::Rename { account, .. }
        | FolderIntent::Move { account, .. }
        | FolderIntent::Trash { account, .. }
        | FolderIntent::Delete { account, .. } => account,
    }
}

/// A unique-per-intent idempotency key for a folder edit, from the shared mint
/// under the family's own prefix.
fn generated_idempotency() -> String {
    crate::mint_idempotency("mailbox-edit")
}

#[cfg(test)]
#[path = "folder_writes_tests.rs"]
mod tests;
