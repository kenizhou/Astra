// SPDX-License-Identifier: GPL-3.0-only
//! Message fixtures and the snapshot row helpers that read one back, split out
//! of the fixture root so it stays under the 500-line limit beside the SMTP
//! leg. Everything here is `pub(crate)`, re-exported by the parent under the
//! `fakes::…` paths every test module already uses.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use engine_api::{EmailAddress, MailboxId, Message};

/// How many folder rows the pane holds across every account.
pub(crate) fn pane_rows(snapshot: &astra_viewmodel::MailboxListSnapshot) -> usize {
    snapshot
        .account_folders
        .iter()
        .map(|account| account.folders.len())
        .sum()
}

/// The folder keys the pane draws for `account`, in order.
pub(crate) fn folder_keys(
    snapshot: &astra_viewmodel::MailboxListSnapshot,
    account: &str,
) -> Vec<String> {
    snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id.as_str() == account)
        .map(|row| {
            row.folders
                .iter()
                .map(|folder| folder.key.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A message `key` filed in `mailbox` about `subject`, from Ada, delivered at a fixed
/// base instant, unread and unflagged. Every list fixture starts here and mutates the
/// axis under test.
pub(crate) fn mail(key: &str, mailbox: &str, subject: &str) -> Message {
    let mut message = Message::new(
        engine_core::ids::MessageId::try_from(key).unwrap(),
        engine_core::membership::Memberships::of_one(MailboxId::try_from(mailbox).unwrap()),
    );
    message.envelope.subject = Some(subject.to_owned());
    message.envelope.from = vec![EmailAddress::named("Ada Lovelace", "ada@example.test")];
    message.received_at = Some(engine_api::UtcDateTime::new(2026, 10, 9, 8, 0, 0).unwrap());
    message
}

/// Delivers `message` at `hour` past the fixture base instant, so list order is fixed
/// without depending on store insertion order.
pub(crate) fn delivered_at(mut message: Message, hour: u8, minute: u8) -> Message {
    message.received_at = Some(engine_api::UtcDateTime::new(2026, 10, 9, hour, minute, 0).unwrap());
    message
}

/// Marks `message` read (the `$seen` keyword), the state a mark-read flips away from.
pub(crate) fn read(mut message: Message) -> Message {
    message
        .keywords
        .insert(engine_api::Keyword::system(engine_api::SystemKeyword::Seen));
    message
}

/// Flags `message` (the `$flagged` keyword).
pub(crate) fn flagged(mut message: Message) -> Message {
    message.keywords.insert(engine_api::Keyword::system(
        engine_api::SystemKeyword::Flagged,
    ));
    message
}

/// Puts `message` on provider-assigned thread `thread`, so several messages group into
/// one conversation the way a JMAP server threads them.
pub(crate) fn threaded(mut message: Message, thread: &str) -> Message {
    message.thread = Some(engine_core::mail::ThreadRef::provider_assigned(
        engine_core::ids::ThreadId::try_from(thread).unwrap(),
    ));
    message
}

/// The message-row keys a snapshot lists, in display order; the shape every list
/// assertion reads before it looks at any row's fields.
pub(crate) fn row_keys(snapshot: &astra_viewmodel::MailboxListSnapshot) -> Vec<String> {
    snapshot
        .rows
        .iter()
        .map(|row| match &row.key {
            astra_viewmodel::RowKey::Message { key, .. } => key.as_str().to_owned(),
            astra_viewmodel::RowKey::Thread { thread_id, .. } => thread_id.as_str().to_owned(),
        })
        .collect()
}

/// The one row a snapshot lists under `key`; panics when it is absent, which is the
/// assertion.
pub(crate) fn row<'a>(
    snapshot: &'a astra_viewmodel::MailboxListSnapshot,
    key: &str,
) -> &'a astra_viewmodel::MessageRowView {
    snapshot
        .rows
        .iter()
        .find(|row| match &row.key {
            astra_viewmodel::RowKey::Message { key: row_key, .. } => row_key.as_str() == key,
            astra_viewmodel::RowKey::Thread { thread_id, .. } => thread_id.as_str() == key,
        })
        .unwrap_or_else(|| panic!("no row keyed {key}"))
}

/// How many times the sending surface has been signalled, for the assertions
/// that count a queue's transitions.
pub(crate) fn sending_signals(surfaces: &Arc<Mutex<Vec<crate::Surface>>>) -> usize {
    surfaces
        .lock()
        .unwrap()
        .iter()
        .filter(|surface| **surface == crate::Surface::Sending)
        .count()
}

/// A message `key` filed in the Sent mailbox, from the account's own identity,
/// so a delivered send's copy appears to a later sync pass the way a real Sent
/// folder would present it.
pub(crate) fn sent_copy(key: &str, draft: &engine_api::Draft) -> Message {
    let mut message = Message::new(
        engine_core::ids::MessageId::try_from(key).unwrap(),
        engine_core::membership::Memberships::of_one(MailboxId::try_from("sent").unwrap()),
    );
    message.envelope.subject = Some(draft.subject.clone());
    message.envelope.from = vec![draft.from.clone()];
    message.envelope.to = draft.to.clone();
    message.envelope.cc = draft.cc.clone();
    message.envelope.message_id = vec![draft.message_id.clone()];
    message
}
