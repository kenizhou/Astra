// SPDX-License-Identifier: GPL-3.0-only
//! The core flag reminder and wake: ask to be notified about a message at a
//! wake instant, then answer a notice when the instant reaches. The message
//! does NOT move and is NOT auto-flagged: it stays put, so the provider key
//! recorded at set time stays a valid link and the wake needs no sync, no
//! re-resolution, and no engine read (the subject and sender are captured at
//! set time). The schedule lives in the device-local [`ReminderStore`] the
//! host installs; the wake rides no mail-write path at all.

use std::sync::Arc;

use astra_reminders::{NewReminder, ReminderStore};
use engine_api::{EmailAddress, Provider};
use thiserror::Error;

use crate::{notifications::NewMailNotice, reference::RowRef, App};

/// Why a reminder or wake could not be applied. The interactive path folds
/// these into a logged skip; the typed form lets the host report them.
#[derive(Debug, Error)]
pub enum ReminderError {
    /// A conversation row cannot be reminded: a reminder names one message.
    #[error("a whole conversation cannot be reminded")]
    ConversationRow,
    /// The row's account is no longer configured.
    #[error("the row's account is no longer configured")]
    UnknownAccount,
    /// The row's account has no provider to read through.
    #[error("the row's account has no provider to read through")]
    NoProvider,
    /// The message key names nothing in the account's store.
    #[error("the message no longer resolves in the store")]
    Unresolved,
    /// The reminder store failed.
    #[error("the reminder store failed: {0}")]
    Store(#[from] astra_reminders::ReminderError),
}

impl<P: Provider> App<P> {
    /// Installs the reminder store the core records and wakes over. Call it
    /// before the first reminder: an app with no store installed records
    /// nothing, the same shape the snooze store installs.
    pub fn install_reminder_store(&self, store: Arc<ReminderStore>) {
        *self.reminders.lock().expect("reminders mutex poisoned") = Some(store);
    }

    /// Reminds one message at `wake_at` (unix seconds): reads the message's
    /// provider key, subject, and sender from the engine's own copy and
    /// records the entry. A conversation row is refused (a reminder names
    /// one message). The message never moves and is never flagged.
    ///
    /// # Errors
    ///
    /// Every [`ReminderError`]: the conversation refusal, the account and
    /// provider refusals, a message that no longer resolves, and the store
    /// failure.
    pub async fn remind_message(&self, row: RowRef, wake_at: i64) -> Result<(), ReminderError> {
        let message = match row {
            RowRef::Message(message) => message,
            RowRef::Thread(_) => return Err(ReminderError::ConversationRow),
        };
        let Some(store) = self
            .reminders
            .lock()
            .expect("reminders mutex poisoned")
            .clone()
        else {
            return Ok(());
        };
        let account = message.account.clone();
        let Some(handle) = self.account_handle(&account).await else {
            return Err(ReminderError::UnknownAccount);
        };
        if handle.providers.is_empty() {
            return Err(ReminderError::NoProvider);
        }
        // The link and the notice's words read from the store's own copy: the
        // message never moves, so the provider key stays valid and the wake
        // needs no re-resolution.
        let stored = self
            .engine
            .messages(&account)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|stored| stored.id.key() == &message.key)
            .ok_or(ReminderError::Unresolved)?;
        let subject = stored.envelope.subject.clone().unwrap_or_default();
        let sender = stored
            .envelope
            .from
            .first()
            .map(|from| from.email.as_str().to_owned())
            .unwrap_or_default();
        store.record(&NewReminder {
            account,
            provider_key: message.key.clone(),
            wake_at,
            subject,
            sender,
        })?;
        Ok(())
    }

    /// Wakes every entry whose wake instant has reached `now`: for each,
    /// builds a [`NewMailNotice`] from the subject and sender captured at set
    /// time (the provider key the link), removes the entry, and answers the
    /// notices. No sync, no re-resolution, no move, no surface republication:
    /// the message stays put. A bad row is skipped by the due read, never
    /// stranding the batch.
    ///
    /// # Errors
    ///
    /// [`ReminderError::Store`] when the due read fails; a per-entry removal
    /// failure is logged and the batch moves on. The body is synchronous: it
    /// reads the store and builds notices, no engine call (the host's port
    /// wraps it in the async tick the timer drives).
    pub fn wake_due_reminders(&self, now: i64) -> Result<Vec<NewMailNotice>, ReminderError> {
        let Some(store) = self
            .reminders
            .lock()
            .expect("reminders mutex poisoned")
            .clone()
        else {
            return Ok(Vec::new());
        };
        let due = store.due(now)?;
        let mut notices = Vec::new();
        for entry in due {
            notices.push(NewMailNotice {
                account: entry.account.clone(),
                key: entry.provider_key.clone(),
                from: Some(EmailAddress::new(entry.sender.clone())),
                subject: Some(entry.subject.clone()),
                preview: None,
            });
            if let Err(err) = store.remove(entry.id) {
                log::warn!("reminders: the wake removal failed: {err}");
            }
        }
        Ok(notices)
    }
}

#[cfg(test)]
#[path = "reminders_tests.rs"]
mod tests;
