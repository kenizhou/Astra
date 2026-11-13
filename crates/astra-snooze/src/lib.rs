// SPDX-License-Identifier: GPL-3.0-only
//! `astra-snooze`: the device-local snooze store.
//!
//! Snooze sets a message aside until a wake instant: the message moves to a
//! Snooze folder and returns to its origin folder when the wake fires. The
//! engine has no snooze mechanism, so the schedule lives here, in a sqlite
//! store the host opens beside the engine store (the AI ledger store's
//! shape: one rusqlite connection under a mutex, embedded migrations, the
//! five-second busy timeout, an in-memory open for tests).
//!
//! One row per snoozed message. The stable identity is the RFC 5322
//! `Message-ID` header ([`MessageIdHeader`]): an IMAP move synthesizes a new
//! provider key in the destination folder, so the stored [`ProviderKey`] is
//! diagnostic only and goes stale on the move out. The wake re-resolves the
//! message by its `Message-ID` after a sync, never by the stored key.
//!
//! The store is device-local: it does not roam across devices, and the CLI
//! does not share it (a host-timer feature).
//!
//! [`MessageIdHeader`]: engine_api::MessageIdHeader
//! [`ProviderKey`]: engine_api::ProviderKey

mod store;

pub use store::{NewSnooze, SnoozeEntry, SnoozeError, SnoozeStore};
