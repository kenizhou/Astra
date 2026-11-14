// SPDX-License-Identifier: GPL-3.0-only
//! `astra-watches`: the device-local watch-replies store.
//!
//! A watch asks to be notified when a conversation gets its first reply. The
//! watch NEVER moves mail and owns no engine state (the locked kylins
//! reference red line): the engine has no watch concept, so the membership
//! lives here, in a sqlite store the host opens beside the engine store (the
//! reminder store's shape: one rusqlite connection under a mutex, embedded
//! migrations, the five-second busy timeout, an in-memory open for tests).
//!
//! One row per watched (account, thread): the owning account, the thread id
//! resolved at set time (the row's handle), and one representative member
//! key captured at set time. A derived thread is named the smallest owned
//! Message-ID in its merged component and re-keys when a reply arrives with a
//! smaller id, so the check re-resolves the member key through the engine at
//! check time rather than trusting the stored thread id. There is no due
//! logic and no timer: the arrival lane is the only trigger, and the one-shot
//! consume rides the same `unwatch` the menu does. The store is device-local:
//! it does not roam across devices, and the CLI does not share it.
//!
//! [`ProviderKey`]: engine_api::ProviderKey

mod store;

pub use store::{NewWatch, WatchEntry, WatchError, WatchStore};
