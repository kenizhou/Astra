// SPDX-License-Identifier: GPL-3.0-only
//! `astra-categories`: the device-local colour category store.
//!
//! A category is a coloured label a user defines once and assigns to
//! conversations. The store holds two tables: the global definitions (a slug
//! id, a case-insensitively unique name, a `#RRGGBB` colour) and one
//! assignment row per (account, thread, category). It is a LOCAL overlay: no
//! engine write, no sync, no protocol concept. EAS and IMAP accounts are
//! served identically because the tables are protocol-agnostic shell state,
//! and the engine's keywords are never touched (a category is not a keyword).
//!
//! The shape mirrors the reminder store: one rusqlite connection under a
//! mutex, embedded migrations claimed in a transaction, the five-second busy
//! timeout on a file open, an in-memory open for tests. Foreign keys are
//! enabled on every open so deleting a definition cascades its assignments
//! away (rusqlite leaves them off by default). The store is device-local: it
//! does not roam across devices, and the CLI does not share it (the
//! snooze/reminders precedent).

mod id;
mod store;

pub use id::CategoryId;
pub use store::{CategoryDef, CategoryError, CategoryStore};
