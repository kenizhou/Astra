// SPDX-License-Identifier: GPL-3.0-only
//! The message-list keyword writes: mark-read and flag, the two state changes a list row
//! drives. Ported from mail-calendar's mail ops in their single-row shapes.
//!
//! Every edit goes through the engine's outbox ([`engine_api::Engine::edit_mail`]): it
//! is queued durably first, which shows the keyword change in the store from that
//! moment, then sent to the provider. The outcome is deliberately not surfaced as a
//! return value: the mutation result is discarded and the refreshed snapshot publishes
//! what actually holds (the flipped row state once queued; the taken-back state only
//! when the server refuses the edit for good).

use engine_api::{MailEdit, Provider, ProviderKey};

use crate::{
    reference::{RowRef, ThreadRef},
    App,
};

impl<P: Provider> App<P> {
    /// Marks one row read or unread on its owning account. A conversation row marks
    /// every message on the thread.
    pub(crate) async fn mark_read(&self, row: RowRef, read: bool) {
        self.edit_row_keywords(row, |key| MailEdit::mark_seen(key, read))
            .await;
    }

    /// Flags or unflags one row on its owning account, a conversation row's every
    /// member included.
    pub(crate) async fn set_flagged(&self, row: RowRef, flagged: bool) {
        self.edit_row_keywords(row, |key| MailEdit::set_flagged(key, flagged))
            .await;
    }

    /// Resolves `row` to its member keys, applies one keyword `edit` per key through the
    /// engine's outbox, then republishes the list past the row cache: the store shows a
    /// queued keyword change at once, so the rebuild carries the flipped row state
    /// before any server answer, and a refusal for good is what shows instead.
    async fn edit_row_keywords(&self, row: RowRef, build: impl Fn(ProviderKey) -> MailEdit) {
        let account = row.account().clone();
        let keys = match &row {
            RowRef::Message(message) => vec![message.key.clone()],
            RowRef::Thread(thread) => self.thread_member_keys(thread).await,
        };
        if keys.is_empty() {
            return;
        }
        // The account handle is cloned out so the guard is released before the provider
        // round-trip; an account the app no longer holds, or one whose provider is
        // gone, cannot be written and is skipped rather than guessed at.
        let Some(handle) = self.account_handle(&account).await else {
            log::warn!("edit: the row's account is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("edit: the row's account has no provider to write through");
            return;
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
    }

    /// Every message key a thread row stands for, straight from the store's thread
    /// index, so a conversation shown in a windowed list still resolves the members the
    /// window cut off.
    ///
    /// A message the server never threaded projects under its own provider key as the
    /// thread id (the list's grouping convention), so the store holds no thread for it
    /// and the index read finds nothing; that lone message is resolved by key instead.
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

/// A unique-per-intent idempotency key for an edit, from the shared mint
/// under the family's own prefix. Unique per call rather than per target, so
/// mark-read then mark-unread of one message never collapses into a single op.
fn generated_idempotency() -> String {
    crate::mint_idempotency("edit")
}
