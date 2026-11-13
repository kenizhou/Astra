// SPDX-License-Identifier: GPL-3.0-only
//! The core snooze and wake: set a message aside until a wake instant, then
//! return it to its origin folder. The engine has no snooze mechanism, so
//! the schedule lives in the device-local [`SnoozeStore`] the host installs
//! and the two moves ride the existing mail-write path.
//!
//! Two facts dictate the shape. An IMAP move synthesizes a NEW provider key
//! in the destination folder, so the wake RE-RESOLVES the message by its
//! `Message-ID` header after a sync; it never reuses the key stored at
//! snooze time (that key is diagnostic only). And there is no Snooze
//! MailboxRole, so snooze CREATES a "Snooze" folder per account and moves
//! into it.

use std::sync::Arc;

use astra_snooze::{NewSnooze, SnoozeEntry, SnoozeStore};
use engine_api::{AccountId, Mailbox, MailboxRole, Message, Provider, ProviderKey};
use thiserror::Error;

use crate::{
    mail_writes::{resolve_move_target, MailWriteError},
    protocol::FolderIntent,
    reference::{FolderRef, MessageRef, RowRef},
    App, FolderWriteError,
};

/// The name of the folder snooze creates and moves into: there is no Snooze
/// MailboxRole, so the folder is a conventional one the core ensures exists.
const SNOOZE_FOLDER: &str = "Snooze";

/// Why a snooze or wake could not be applied. The interactive path folds
/// these into a logged skip; the typed form lets the host report them.
#[derive(Debug, Error)]
pub enum SnoozeError {
    /// A conversation row cannot be snoozed: a move names messages only.
    #[error("a whole conversation cannot be snoozed")]
    ConversationRow,
    /// The row's account is no longer configured.
    #[error("the row's account is no longer configured")]
    UnknownAccount,
    /// The row's account has no provider to write through.
    #[error("the row's account has no provider to write through")]
    NoProvider,
    /// The message carries no `Message-ID` header, the stable identity the
    /// wake re-resolves by.
    #[error("the message carries no Message-ID to snooze by")]
    NoMessageId,
    /// The message key names nothing in the account's store.
    #[error("the message no longer resolves in the store")]
    Unresolved,
    /// The account's Snooze folder could not be created or resolved.
    #[error("the account has no Snooze folder to move into")]
    NoSnoozeFolder,
    /// The snooze store failed.
    #[error("the snooze store failed: {0}")]
    Store(#[from] astra_snooze::SnoozeError),
    /// The move failed.
    #[error("the move failed: {0}")]
    Move(#[from] MailWriteError),
    /// The folder create failed.
    #[error("the folder create failed: {0}")]
    Folder(#[from] FolderWriteError),
}

impl<P: Provider> App<P> {
    /// Installs the snooze store the core records and wakes over. Call it
    /// before the first snooze: an app with no store installed records
    /// nothing, the same shape the analytics ledger installs.
    pub fn install_snooze_store(&self, store: Arc<SnoozeStore>) {
        *self.snooze.lock().expect("snooze mutex poisoned") = Some(store);
    }

    /// Snoozes one message until `wake_at` (unix seconds): ensures the
    /// account's Snooze folder exists, records the stable `Message-ID` and
    /// the origin folder, then moves the message into the Snooze folder. A
    /// conversation row is refused (a move names messages only).
    ///
    /// # Errors
    ///
    /// Every [`SnoozeError`]: the conversation refusal, the account and
    /// provider refusals, a message with no `Message-ID` or no store row, a
    /// Snooze folder that cannot be created, and the store and move failures.
    pub async fn snooze_message(&self, row: RowRef, wake_at: i64) -> Result<(), SnoozeError> {
        let message = match row {
            RowRef::Message(message) => message,
            RowRef::Thread(_) => return Err(SnoozeError::ConversationRow),
        };
        let Some(store) = self.snooze.lock().expect("snooze mutex poisoned").clone() else {
            return Ok(());
        };
        let account = message.account.clone();
        let Some(handle) = self.account_handle(&account).await else {
            return Err(SnoozeError::UnknownAccount);
        };
        if handle.providers.is_empty() {
            return Err(SnoozeError::NoProvider);
        }
        // The stable identity and the origin folder read from the store's own
        // copy: the provider key travels for diagnostics only, never the wake.
        let stored = self
            .engine
            .messages(&account)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|stored| stored.id.key() == &message.key)
            .ok_or(SnoozeError::Unresolved)?;
        let message_id = stored
            .envelope
            .message_id
            .first()
            .cloned()
            .ok_or(SnoozeError::NoMessageId)?;
        let origin_folder = stored
            .mailboxes
            .iter()
            .next()
            .map(|mailbox| mailbox.as_str().to_owned())
            .ok_or(SnoozeError::Unresolved)?;
        let subject = stored.envelope.subject.clone().unwrap_or_default();
        let sender = stored
            .envelope
            .from
            .first()
            .map(|from| from.email.as_str().to_owned())
            .unwrap_or_default();
        let snooze_key = self.ensure_snooze_folder(&account).await?;
        store.record(&NewSnooze {
            account: account.clone(),
            message_id,
            provider_key: message.key.clone(),
            wake_at,
            origin_folder,
            subject,
            sender,
        })?;
        self.move_messages(
            vec![message],
            FolderRef {
                account,
                key: snooze_key,
            },
        )
        .await?;
        Ok(())
    }

    /// Wakes every entry whose wake instant has reached `now`: for each, syncs
    /// the account so the moved copy reconciles, RE-RESOLVES the message by
    /// its `Message-ID` (subject and sender as fallback) within the Snooze
    /// folder, moves it back to its origin folder (or the Inbox when the
    /// origin is gone), and removes the entry. The move republishes the
    /// mailbox list, so the web learns of the wake through the existing
    /// surface loop. Answers how many entries were processed.
    ///
    /// # Errors
    ///
    /// [`SnoozeError::Store`] when the due read fails; a per-entry failure is
    /// logged and the batch moves on, never stranding the remaining wakes.
    pub async fn wake_due_snoozes(&self, now: i64) -> Result<usize, SnoozeError> {
        let Some(store) = self.snooze.lock().expect("snooze mutex poisoned").clone() else {
            return Ok(0);
        };
        let due = store.due(now)?;
        let mut woken = 0;
        for entry in due {
            self.wake_one(&store, &entry).await;
            woken += 1;
        }
        Ok(woken)
    }

    /// Ensures the account's Snooze folder exists, creating it when absent,
    /// and answers its key. There is no Snooze role, so the folder is found
    /// by its conventional name.
    async fn ensure_snooze_folder(&self, account: &AccountId) -> Result<String, SnoozeError> {
        let tree = self.engine.mailboxes(account).await.unwrap_or_default();
        if let Some(key) = snooze_folder_key(&tree) {
            return Ok(key);
        }
        self.write_folder(FolderIntent::Create {
            account: account.clone(),
            parent: None,
            name: SNOOZE_FOLDER.to_owned(),
        })
        .await?;
        let tree = self.engine.mailboxes(account).await.unwrap_or_default();
        snooze_folder_key(&tree).ok_or(SnoozeError::NoSnoozeFolder)
    }

    /// Wakes one entry: syncs the account so the moved copy reconciles,
    /// re-resolves the message by `Message-ID` (subject and sender as
    /// fallback) within the Snooze folder, moves it home, and removes the
    /// entry. A message or folder that no longer resolves is a stale entry:
    /// it is removed without a move, never stranding the batch.
    async fn wake_one(&self, store: &SnoozeStore, entry: &SnoozeEntry) {
        let account = &entry.account;
        self.sync_account(account).await;
        let tree = self.engine.mailboxes(account).await.unwrap_or_default();
        let Some(snooze_key) = snooze_folder_key(&tree) else {
            let _ = store.remove(entry.id);
            return;
        };
        let messages = self.engine.messages(account).await.unwrap_or_default();
        let Some(key) = resolve_snoozed(&messages, &snooze_key, entry) else {
            let _ = store.remove(entry.id);
            return;
        };
        // The origin folder when it still stands, else the Inbox: a wake
        // never strands a message in the Snooze folder over a gone origin.
        let destination = if tree
            .iter()
            .any(|mailbox| mailbox.id.as_str() == entry.origin_folder)
        {
            entry.origin_folder.clone()
        } else {
            resolve_move_target(&tree, &MailboxRole::Inbox)
                .map_or_else(|| "INBOX".to_owned(), |mailbox| mailbox.as_str().to_owned())
        };
        let message = MessageRef {
            account: account.clone(),
            key,
        };
        if let Err(err) = self
            .move_messages(
                vec![message],
                FolderRef {
                    account: account.clone(),
                    key: destination,
                },
            )
            .await
        {
            log::warn!("snooze: the wake move failed: {err}");
        }
        let _ = store.remove(entry.id);
    }
}

/// The key of the account's Snooze folder, matched by its conventional name
/// (the leaf, case-insensitively): there is no Snooze role to resolve by.
fn snooze_folder_key(tree: &[Mailbox]) -> Option<String> {
    tree.iter()
        .find(|mailbox| {
            mailbox
                .name
                .rsplit(['/', '.', '\\'])
                .next()
                .unwrap_or(&mailbox.name)
                .trim()
                .eq_ignore_ascii_case(SNOOZE_FOLDER)
        })
        .map(|mailbox| mailbox.id.as_str().to_owned())
}

/// Re-resolves the snoozed message within the Snooze folder by its stable
/// `Message-ID`, falling back to the subject and sender when the header
/// moved. NEVER the stored provider key: the move synthesized a new one.
fn resolve_snoozed(
    messages: &[Message],
    snooze_key: &str,
    entry: &SnoozeEntry,
) -> Option<ProviderKey> {
    let in_snooze = || {
        messages.iter().filter(|message| {
            message
                .mailboxes
                .iter()
                .any(|mailbox| mailbox.as_str() == snooze_key)
        })
    };
    if let Some(message) = in_snooze().find(|message| {
        message
            .envelope
            .message_id
            .iter()
            .any(|id| id == &entry.message_id)
    }) {
        return Some(message.id.key().clone());
    }
    in_snooze()
        .find(|message| {
            message.envelope.subject.as_deref() == Some(entry.subject.as_str())
                && message
                    .envelope
                    .from
                    .first()
                    .map(|from| from.email.as_str())
                    == Some(entry.sender.as_str())
        })
        .map(|message| message.id.key().clone())
}

#[cfg(test)]
#[path = "snooze_tests.rs"]
mod tests;
