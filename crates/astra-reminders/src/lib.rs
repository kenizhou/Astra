// SPDX-License-Identifier: GPL-3.0-only
//! `astra-reminders`: the device-local flag reminder store.
//!
//! A flag reminder asks to be notified about a message at a wake instant.
//! The message does NOT move and is NOT auto-flagged: it stays put, so the
//! provider key recorded at set time stays a valid link and the wake needs
//! no re-resolution (the snooze store's `Message-ID` machinery is absent
//! here by design). The engine has no reminder mechanism, so the schedule
//! lives here, in a sqlite store the host opens beside the engine store
//! (the AI ledger store's shape: one rusqlite connection under a mutex,
//! embedded migrations, the five-second busy timeout, an in-memory open for
//! tests).
//!
//! One row per reminder: the owning account, the provider key, the wake
//! instant, and the subject and sender captured at set time so the wake
//! builds its notice without an engine read. The store is device-local: it
//! does not roam across devices, and the CLI does not share it (a host-timer
//! feature, the snooze precedent).
//!
//! [`ProviderKey`]: engine_api::ProviderKey

mod store;

pub use store::{NewReminder, ReminderEntry, ReminderError, ReminderStore};
