// SPDX-License-Identifier: GPL-3.0-only
//! The reading pane's pipeline: one open message resolved in its account, its
//! body fetched through the engine and sanitised, and the snapshot published on
//! [`Surface::Reading`](crate::Surface::Reading). An `impl App` block for the
//! open, close, and refresh-republish arms, and one free function that does the
//! fetching, so the pipeline is testable against a bare engine and provider the
//! way the engine's own body tests are.
//!
//! The port source is mail-calendar's reading path, reshaped two ways. Astra
//! has one reading pane rather than per-window readers, so there is no reader
//! id and one `Surfaced` cell holds the whole state. And Astra's snapshot has
//! no load-error flag (the SP2c wire is pinned): a message that cannot be
//! resolved, or whose body cannot be fetched, renders nothing and the pane
//! stays empty, which is the honest answer over a snapshot that would claim the
//! message carries no body at all.

use astra_viewmodel::{AttachmentRow, MessageKey, ReadingSnapshot};
use engine_api::{Engine, Message, MessageAttachment, Provider, SystemKeyword};

use crate::{
    list_rows::format_instant,
    reference::{MessageRef, RowRef},
    sanitise, App,
};

impl<P: Provider> App<P> {
    /// The reading snapshot, for the host's pull after a
    /// [`Surface::Reading`](crate::Surface::Reading) signal: the one open
    /// message's sanitised body, or `None` when nothing is open.
    #[must_use]
    pub fn reading(&self) -> Option<ReadingSnapshot> {
        self.reading.get()
    }

    /// Opens one message in the reading pane: renders its body through the
    /// pipeline, publishes the snapshot, and marks the message read through the
    /// same write path [`Intent::MarkRead`](crate::Intent::MarkRead) drives.
    ///
    /// The read state is re-resolved at the moment of that decision, the way
    /// mail-calendar's open does, so a read that landed while the body was in
    /// flight (another intent, another client of the same mailbox) is not edited
    /// again. A message the store no longer holds opens nothing: the pane is
    /// cleared rather than left showing whatever was open before. The open is
    /// claimed before the first `await`, so a close or a newer open that lands
    /// while the body is in flight drops the render that arrives after it: an
    /// older open must not win by finishing last.
    pub(crate) async fn open_message(&self, message: MessageRef) {
        *self
            .open_reading
            .lock()
            .expect("open-reading mutex poisoned") = Some(message.clone());
        match self.render_open(&message).await {
            Some(snapshot) => {
                if self.still_open(&message) {
                    self.reading.publish(Some(snapshot));
                    if self.message_unread(&message).await {
                        self.mark_read(RowRef::Message(message), true).await;
                    }
                }
            }
            None => {
                if self.still_open(&message) {
                    self.clear_reading();
                }
            }
        }
    }

    /// Shuts the reading pane: the open message and its body are forgotten, and
    /// the published snapshot goes empty.
    pub(crate) fn close_reading(&self) {
        self.clear_reading();
    }

    /// Re-renders the open message after a refresh republishes everything else:
    /// the pane shows the store's fresh copy, or closes when the message is
    /// gone. Marks nothing read here; the open already did, and a still-open
    /// pane that refreshed is a pane that was read.
    pub(crate) async fn republish_reading(&self) {
        let Some(message) = self
            .open_reading
            .lock()
            .expect("open-reading mutex poisoned")
            .clone()
        else {
            return;
        };
        match self.render_open(&message).await {
            Some(snapshot) => {
                if self.still_open(&message) {
                    self.reading.publish(Some(snapshot));
                }
            }
            None => {
                if self.still_open(&message) {
                    self.clear_reading();
                }
            }
        }
    }

    /// Whether `message` is unread **right now**, re-resolved from the store:
    /// the mark-read gate of an open, read at the moment of the decision the way
    /// mail-calendar reads it, so the state the render began with is not the
    /// state the edit is decided on. `false` for a message the store no longer
    /// holds: there is nothing left to edit.
    async fn message_unread(&self, message: &MessageRef) -> bool {
        resolve(&self.engine, message)
            .await
            .is_some_and(|original| !original.has_system_keyword(SystemKeyword::Seen))
    }

    /// Whether `message` is still what the pane has open: the guard that keeps
    /// a slower render of a superseded open (or of an already-closed pane) from
    /// publishing over a newer state. The reading sibling of the list's
    /// generation counter, an identity check because one pane holds one message.
    fn still_open(&self, message: &MessageRef) -> bool {
        self.open_reading
            .lock()
            .expect("open-reading mutex poisoned")
            .as_ref()
            == Some(message)
    }

    /// Publishes the empty reading snapshot and forgets the open message, the
    /// one path every "nothing is open" state goes through.
    fn clear_reading(&self) {
        *self
            .open_reading
            .lock()
            .expect("open-reading mutex poisoned") = None;
        self.reading.publish(None);
    }

    /// Renders `message` through this app's engine and its account's first
    /// provider; `None` when the account is gone, holds no provider, or the
    /// message does not resolve.
    async fn render_open(&self, message: &MessageRef) -> Option<ReadingSnapshot> {
        let handle = self.account_handle(&message.account).await?;
        let provider = handle.providers.first()?;
        render_reading(&self.engine, provider, message).await
    }
}

/// Renders `message` (its account and provider key bound together) into the
/// reading snapshot: resolve it in the store, fetch the body through the
/// engine's cache-first reads, sanitise the HTML and resolve its inline `cid:`
/// images into `data:` URIs, gate every remote reference behind placeholders,
/// and list the attachment metadata. `None` when the message is not in the
/// account's synced set or its body cannot be fetched.
///
/// This is the non-mutating read path; the mark-read is the caller's decision,
/// not the pipeline's.
pub(crate) async fn render_reading<P: Provider>(
    engine: &Engine,
    provider: &P,
    message: &MessageRef,
) -> Option<ReadingSnapshot> {
    let original = resolve(engine, message).await?;
    let body = engine
        .message_body(provider, &message.account, &original)
        .await
        .map_err(|err| {
            log::warn!(
                "reading: the body fetch failed for account {}: {err}",
                message.account.as_str(),
            );
            err
        })
        .ok()?;
    // The HTML is the engine's raw, unsanitised part. Sanitise first (the cheap
    // pass that also tells us whether the body references inline images), fetch
    // the parts only when it does, then resolve them into the body and gate the
    // remote references. `plain` is the text view and passes through untouched.
    let (html, remote_image_count) = match body.html() {
        Some(raw) => {
            let cleaned = sanitise::sanitise(raw);
            let parts = if cleaned.has_cid_references {
                engine
                    .message_inline_parts(provider, &message.account, &original)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let inlined = sanitise::inline_cid_images(&cleaned.html, &parts);
            let gated = sanitise::gate_remote_sources(&inlined);
            (Some(gated.html), gated.remote_image_count)
        }
        None => (None, 0),
    };
    let attachments = engine
        .message_attachments(provider, &message.account, &original)
        .await
        .unwrap_or_default()
        .iter()
        .map(attachment_row)
        .collect();
    Some(ReadingSnapshot {
        message: MessageKey {
            account: message.account.clone(),
            key: message.key.clone(),
        },
        subject: original.envelope.subject.clone().unwrap_or_default(),
        // The sender the reading header names, split the way the snapshot
        // carries it: the display name (the bare address when the header
        // carried none) beside the address itself.
        from_name: sender_name(&original),
        from_address: original
            .envelope
            .from
            .first()
            .map_or_else(String::new, |from| from.email.clone()),
        // The same instant the list row is ordered by: the delivery date,
        // falling back to the Date header, whole seconds in UTC.
        sent_at: original
            .received_at
            .or(original.sent_at)
            .map_or_else(String::new, format_instant),
        html,
        plain: body.plain().map(str::to_owned),
        attachments,
        remote_image_count,
    })
}

/// Resolves `message` to its stored [`Message`], the read every body fetch is
/// addressed through (it carries the id and blob handle the provider needs).
/// `None` when the account's synced set holds no such key: moved, tombstoned,
/// or never synced.
async fn resolve(engine: &Engine, message: &MessageRef) -> Option<Message> {
    engine
        .messages_by_keys(&message.account, std::slice::from_ref(&message.key))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

/// The sender's display name, falling back to their bare address when the
/// header carried no usable name, and empty when there is no sender at all:
/// the same rule the list row's name follows, so the pane and the row never
/// disagree about who a message is from.
fn sender_name(message: &Message) -> String {
    message
        .envelope
        .from
        .first()
        .map_or_else(String::new, |from| {
            from.name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(&from.email)
                .to_owned()
        })
}

/// One downloadable attachment as the snapshot's metadata row: the bytes stay
/// with the engine until a save names the row's id.
fn attachment_row(attachment: &MessageAttachment) -> AttachmentRow {
    AttachmentRow {
        id: attachment.id().as_u32(),
        file_name: attachment.file_name().to_owned(),
        media_type: attachment.media_type().to_owned(),
        size: attachment.size(),
    }
}

#[cfg(test)]
#[path = "reading_tests.rs"]
mod tests;
