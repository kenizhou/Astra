// SPDX-License-Identifier: GPL-3.0-only
//! The Astra Mail application core: the protocol of the unidirectional loop.
//!
//! A host observes a [`Surface`] through [`AppObserver`], pulls an immutable snapshot for
//! it, and dispatches an [`Intent`] back; state flows one way and commands the other. The
//! crate root re-exports every protocol type a host needs.

mod protocol;
mod reference;
mod surfaced;

pub use protocol::{AppObserver, FolderIntent, Intent, Surface, ViewMode};
pub use reference::FolderRef;
pub use surfaced::Surfaced;
